use secp256k1::{rand::rngs::OsRng, Scalar, Secp256k1, SecretKey};
use spora_adaptor::{
    complete_signature, create_proof, create_secret_and_point, make_partial_signature, recover_secret, verify_proof, verify_secret,
    AdaptorSecret,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let mut rng = OsRng;

    let (adaptor_secret, adaptor_point) = create_secret_and_point(&secp);
    let proof = create_proof(&secp, &adaptor_secret.0, &adaptor_point)?;
    verify_proof(&secp, &adaptor_point, &proof)?;

    let final_signature_scalar = Scalar::from(SecretKey::new(&mut rng));
    let final_signature_bytes = final_signature_scalar.to_be_bytes();
    let partial_signature = make_partial_signature(&final_signature_scalar, &adaptor_secret.0)?;
    let completed_signature = complete_signature(&partial_signature.0, &adaptor_secret.0)?;
    assert_eq!(completed_signature, final_signature_scalar);

    let recovered_secret = recover_secret(&final_signature_bytes, &partial_signature.0)?;
    assert_eq!(recovered_secret, adaptor_secret.0);
    assert!(verify_secret(&secp, &AdaptorSecret(recovered_secret), &adaptor_point));

    println!("Adaptor roundtrip complete for point {}", hex::encode(adaptor_point.0.serialize()));
    Ok(())
}
