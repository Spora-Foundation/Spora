//! pstt roles.

/// Initializes the pstt with 0 inputs and 0 outputs.
/// Reference: [BIP-370: Creator](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#creator)
#[derive(Debug)]
pub enum Creator {}

/// Adds inputs and outputs to the pstt.
/// Reference: [BIP-370: Constructor](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#constructor)
#[derive(Debug)]
pub enum Constructor {}

/// Can set the sequence number.
/// Reference: [BIP-370: Updater](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#updater)
#[derive(Debug)]
pub enum Updater {}

/// Creates cryptographic signatures for the inputs using private keys.
/// Reference: [BIP-370: Signer](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#signer)
#[derive(Debug)]
pub enum Signer {}

/// Merges multiple pstts into one.
/// Reference: [BIP-174: Combiner](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki#combiner)
#[derive(Debug)]
pub enum Combiner {}

/// Completes the pstt, ensuring all inputs have valid signatures, and finalizes the transaction.
/// Reference: [BIP-174: Input Finalizer](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki#input-finalizer)
#[derive(Debug)]
pub enum Finalizer {}

/// Extracts the final transaction from the pstt once all parts are in place and the pstt is fully signed.
/// Reference: [BIP-370: Transaction Extractor](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#transaction-extractor)
#[derive(Debug)]
pub enum Extractor {}
