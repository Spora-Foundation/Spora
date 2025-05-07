use blake3::hash;

fn main() {
    let hash_empty = hash(b"");
    let hash_abc = hash(b"abc");

    println!("Empty hash: {:?}", hash_empty);
    println!("abc hash: {:?}", hash_abc);
}