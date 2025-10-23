// SPDX-License-Identifier: ISC
// Copyright (C) 2025 Spora developers
//
// Data availability storage: segments and proofs

pub mod proof;
pub mod segment;

pub use proof::{ProofVerifier, SegmentProof};
pub use segment::{SegmentMeta, SegmentReader, SegmentWriter};
