use secp256k1::{rand::rngs::OsRng, Scalar, Secp256k1, SecretKey};
use spora_adaptor::{
    complete_signature, create_proof, create_secret_and_point, make_partial_signature, recover_secret, AdaptorError, Scalar32,
};

fn expect_scalar_out_of_range(result: Result<impl Sized, AdaptorError>) {
    match result {
        Err(AdaptorError::ScalarOutOfRange) => {}
        Err(error) => panic!("expected ScalarOutOfRange, got {error:?}"),
        Ok(_) => panic!("expected ScalarOutOfRange, got Ok"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let secp = Secp256k1::new();
    let mut rng = OsRng;
    let (adaptor_secret, adaptor_point) = create_secret_and_point(&secp);
    let final_signature_scalar = Scalar::from(SecretKey::new(&mut rng));
    let partial_signature = make_partial_signature(&final_signature_scalar, &adaptor_secret.0)?;

    let curve_order: Scalar32 = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xba, 0xae, 0xdc, 0xe6, 0xaf,
        0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
    ];

    expect_scalar_out_of_range(create_proof(&secp, &curve_order, &adaptor_point));
    expect_scalar_out_of_range(make_partial_signature(&final_signature_scalar, &curve_order));
    expect_scalar_out_of_range(complete_signature(&partial_signature.0, &curve_order));
    expect_scalar_out_of_range(complete_signature(&curve_order, &adaptor_secret.0));
    expect_scalar_out_of_range(recover_secret(&curve_order, &partial_signature.0));
    expect_scalar_out_of_range(recover_secret(&final_signature_scalar.to_be_bytes(), &curve_order));

    println!("Adaptor non-canonical scalar rejection example passed");
    Ok(())
}
