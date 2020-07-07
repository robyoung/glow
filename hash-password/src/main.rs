use std::env;

use argon2::{self, Config};

fn main() {
    let command: String = env::args()
        .nth(1)
        .expect("requires a command [encode|verify]");
    let password: String = env::args().nth(2).expect("requires a password to hash");
    eprintln!("Entered password: {}", password);

    match command.as_str() {
        "encode" => {
            let salt: Vec<u8> =
                base64::decode(&env::args().nth(3).expect("requires a salt for the hash"))
                    .expect("salt must be base64 encoded");

            let config = Config::default();
            let hash = argon2::hash_encoded(password.as_bytes(), &salt, &config).unwrap();

            println!("{}", base64::encode(&hash));
        }
        "verify" => {
            let hash_as_vec =
                base64::decode(&env::args().nth(3).expect("requires a hash to verify"))
                    .expect("hash must be base64 encoded");
            let hash = std::str::from_utf8(&hash_as_vec).expect("hash must be valid utf8");
            println!("{:?}", argon2::verify_encoded(&hash, password.as_bytes()));
        }
        _ => {
            panic!("invalid command");
        }
    };
}
