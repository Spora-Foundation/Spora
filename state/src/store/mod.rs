// SPDX-License-Identifier: ISC
// Copyright (C) 2024 Tondi developers
//
// Data availability storage: segments and proofs

pub mod segment;
pub mod proof;

pub use segment::{SegmentWriter, SegmentReader, SegmentMeta};
pub use proof::{SegmentProof, ProofVerifier};

