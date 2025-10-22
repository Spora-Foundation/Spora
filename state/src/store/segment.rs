// SPDX-License-Identifier: ISC
// Copyright (C) 2025Tondi developers
//
// Segment storage: 1GB data segments for DA layer

use crate::{Result, StateError};
use borsh::{BorshDeserialize, BorshSerialize};
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Segment size: 1GB
const SEGMENT_SIZE: u64 = 1024 * 1024 * 1024;

/// Maximum segments in memory before forcing seal
const MAX_OPEN_SEGMENTS: usize = 8;

/// Segment metadata
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SegmentMeta {
    /// Segment ID
    pub segment_id: u32,
    /// Size (bytes written)
    pub size: u64,
    /// Number of Cells
    pub cell_count: u32,
    /// Merkle root (DA commitment)
    pub merkle_root: [u8; 32],
    /// Is sealed?
    pub sealed: bool,
    /// Creation timestamp
    pub created_at: u64,
    /// Sealed timestamp
    pub sealed_at: Option<u64>,
}

/// Segment writer (append-only)
///
/// Manages sequential writes to 1GB segment files
pub struct SegmentWriter {
    /// Base directory for segments
    base_dir: PathBuf,
    /// Current segment ID
    current_segment_id: u32,
    /// Current segment file
    current_file: Arc<Mutex<Option<File>>>,
    /// Current offset
    current_offset: Arc<Mutex<u64>>,
    /// Segment metadata
    segments: Arc<Mutex<Vec<SegmentMeta>>>,
}

impl SegmentWriter {
    /// Create a new segment writer
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_dir)?;
        
        // Find highest existing segment ID
        let max_id = Self::find_max_segment_id(&base_dir)?;
        
        Ok(Self {
            base_dir,
            current_segment_id: max_id.map(|id| id + 1).unwrap_or(0),
            current_file: Arc::new(Mutex::new(None)),
            current_offset: Arc::new(Mutex::new(0)),
            segments: Arc::new(Mutex::new(Vec::new())),
        })
    }
    
    /// Append Cell data to current segment
    ///
    /// Returns: (segment_id, offset, length)
    pub fn append(&self, data: &[u8]) -> Result<(u32, u64, u32)> {
        let mut file_guard = self.current_file.lock();
        let mut offset_guard = self.current_offset.lock();
        
        // Open new segment if needed
        if file_guard.is_none() || *offset_guard + data.len() as u64 > SEGMENT_SIZE {
            drop(file_guard);
            drop(offset_guard);
            self.rotate_segment()?;
            file_guard = self.current_file.lock();
            offset_guard = self.current_offset.lock();
        }
        
        let file = file_guard.as_mut()
            .ok_or_else(|| StateError::Database("No active segment file".to_string()))?;
        
        let segment_id = self.current_segment_id;
        let offset = *offset_guard;
        let length = data.len() as u32;
        
        // Write data
        file.write_all(data)?;
        file.sync_data()?; // Ensure durability
        
        *offset_guard += data.len() as u64;
        
        Ok((segment_id, offset, length))
    }
    
    /// Seal current segment (finalize and compute commitment)
    pub fn seal(&self) -> Result<SegmentMeta> {
        let file_guard = self.current_file.lock();
        let offset_guard = self.current_offset.lock();
        
        if file_guard.is_none() {
            return Err(StateError::Database("No active segment to seal".to_string()));
        }
        
        let segment_id = self.current_segment_id;
        let size = *offset_guard;
        
        // Compute Merkle root (simplified: blake3 hash of entire segment)
        drop(file_guard);
        drop(offset_guard);
        
        let merkle_root = self.compute_merkle_root(segment_id, size)?;
        
        let meta = SegmentMeta {
            segment_id,
            size,
            cell_count: 0, // TODO: track cell count
            merkle_root,
            sealed: true,
            created_at: Self::current_timestamp(),
            sealed_at: Some(Self::current_timestamp()),
        };
        
        // Save metadata
        self.save_segment_meta(&meta)?;
        
        // Close current segment
        let mut file_guard = self.current_file.lock();
        *file_guard = None;
        
        Ok(meta)
    }
    
    /// Force seal and rotate to new segment
    fn rotate_segment(&self) -> Result<()> {
        // Seal current if exists
        if self.current_file.lock().is_some() {
            self.seal()?;
        }
        
        // Open new segment
        let new_id = self.current_segment_id + 1;
        let path = self.segment_path(new_id);
        
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&path)?;
        
        let mut file_guard = self.current_file.lock();
        *file_guard = Some(file);
        
        let mut offset_guard = self.current_offset.lock();
        *offset_guard = 0;
        
        Ok(())
    }
    
    /// Compute Merkle root for segment
    fn compute_merkle_root(&self, segment_id: u32, size: u64) -> Result<[u8; 32]> {
        let path = self.segment_path(segment_id);
        let mut file = File::open(path)?;
        
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB chunks
        let mut total_read = 0u64;
        
        while total_read < size {
            let to_read = std::cmp::min(buffer.len(), (size - total_read) as usize);
            let n = file.read(&mut buffer[..to_read])?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total_read += n as u64;
        }
        
        Ok(*hasher.finalize().as_bytes())
    }
    
    /// Save segment metadata
    fn save_segment_meta(&self, meta: &SegmentMeta) -> Result<()> {
        let path = self.segment_meta_path(meta.segment_id);
        let data = borsh::to_vec(meta)
            .map_err(|e| StateError::Serialization(e.to_string()))?;
        std::fs::write(path, data)?;
        Ok(())
    }
    
    /// Get segment file path
    fn segment_path(&self, segment_id: u32) -> PathBuf {
        self.base_dir.join(format!("segment_{:08}.dat", segment_id))
    }
    
    /// Get segment metadata path
    fn segment_meta_path(&self, segment_id: u32) -> PathBuf {
        self.base_dir.join(format!("segment_{:08}.meta", segment_id))
    }
    
    /// Find maximum existing segment ID
    fn find_max_segment_id(base_dir: &Path) -> Result<Option<u32>> {
        let entries = std::fs::read_dir(base_dir)?;
        let mut max_id = None;
        
        for entry in entries {
            let entry = entry?;
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();
            
            if filename_str.starts_with("segment_") && filename_str.ends_with(".dat") {
                if let Some(id_str) = filename_str.strip_prefix("segment_").and_then(|s| s.strip_suffix(".dat")) {
                    if let Ok(id) = id_str.parse::<u32>() {
                        max_id = Some(max_id.unwrap_or(0).max(id));
                    }
                }
            }
        }
        
        Ok(max_id)
    }
    
    /// Get current Unix timestamp
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

/// Segment reader (random access)
pub struct SegmentReader {
    /// Base directory
    base_dir: PathBuf,
    /// Open files cache
    files: Arc<Mutex<lru::LruCache<u32, File>>>,
}

impl SegmentReader {
    /// Create a new segment reader
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        Ok(Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            files: Arc::new(Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(MAX_OPEN_SEGMENTS).unwrap()
            ))),
        })
    }
    
    /// Read data from a segment
    pub fn read(&self, segment_id: u32, offset: u64, length: u32) -> Result<Vec<u8>> {
        let mut files = self.files.lock();
        
        // Get or open file
        let file = if let Some(f) = files.get(&segment_id) {
            f
        } else {
            let path = self.segment_path(segment_id);
            let file = File::open(path).map_err(|_| StateError::SegmentNotFound(segment_id))?;
            files.put(segment_id, file);
            files.get(&segment_id).unwrap()
        };
        
        // Read data
        let mut buffer = vec![0u8; length as usize];
        let mut file_clone = file.try_clone()?;
        file_clone.seek(SeekFrom::Start(offset))?;
        file_clone.read_exact(&mut buffer)?;
        
        Ok(buffer)
    }
    
    /// Load segment metadata
    pub fn load_meta(&self, segment_id: u32) -> Result<SegmentMeta> {
        let path = self.segment_meta_path(segment_id);
        let data = std::fs::read(path).map_err(|_| StateError::SegmentNotFound(segment_id))?;
        SegmentMeta::try_from_slice(&data)
            .map_err(|e| StateError::Serialization(e.to_string()))
    }
    
    fn segment_path(&self, segment_id: u32) -> PathBuf {
        self.base_dir.join(format!("segment_{:08}.dat", segment_id))
    }
    
    fn segment_meta_path(&self, segment_id: u32) -> PathBuf {
        self.base_dir.join(format!("segment_{:08}.meta", segment_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_segment_writer_append() {
        let tmp = TempDir::new().unwrap();
        let writer = SegmentWriter::new(tmp.path()).unwrap();
        
        let data = vec![0xAA; 1024];
        let (seg_id, offset, length) = writer.append(&data).unwrap();
        
        assert_eq!(seg_id, 0);
        assert_eq!(offset, 0);
        assert_eq!(length, 1024);
    }

    #[test]
    fn test_segment_seal() {
        let tmp = TempDir::new().unwrap();
        let writer = SegmentWriter::new(tmp.path()).unwrap();
        
        let data = vec![0xBB; 2048];
        writer.append(&data).unwrap();
        
        let meta = writer.seal().unwrap();
        assert_eq!(meta.segment_id, 0);
        assert_eq!(meta.size, 2048);
        assert!(meta.sealed);
    }

    #[test]
    fn test_segment_reader() {
        let tmp = TempDir::new().unwrap();
        let writer = SegmentWriter::new(tmp.path()).unwrap();
        
        let data = vec![0xCC; 512];
        let (seg_id, offset, length) = writer.append(&data).unwrap();
        writer.seal().unwrap();
        
        let reader = SegmentReader::new(tmp.path()).unwrap();
        let read_data = reader.read(seg_id, offset, length).unwrap();
        
        assert_eq!(read_data, data);
    }

    #[test]
    fn test_segment_rotation() {
        let tmp = TempDir::new().unwrap();
        let writer = SegmentWriter::new(tmp.path()).unwrap();
        
        // Write small data to first segment
        let data1 = vec![0x11; 1024];
        let (seg1, _, _) = writer.append(&data1).unwrap();
        
        // Seal and rotate
        writer.seal().unwrap();
        
        // Write to second segment
        let data2 = vec![0x22; 1024];
        let (seg2, _, _) = writer.append(&data2).unwrap();
        
        assert_ne!(seg1, seg2);
        assert_eq!(seg2, seg1 + 1);
    }
}

