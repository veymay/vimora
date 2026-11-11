//! Release signing — developer machine only. The private key never goes into the repository.
//!
//!   cargo run -q -p vimora-update --example sign -- keygen
//!       creates the key (~/.config/vimora-release/release.key) and prints its public half
//!   cargo run -q -p vimora-update --example sign -- sign dist/SHA256SUMS
//!       writes dist/SHA256SUMS.sig
//!
//! `VIMORA_RELEASE_KEY=<file>` uses another key file. Keep a copy of the key somewhere safe: the
//! installed computers accept only updates signed with it.

use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use std::path::PathBuf;

fn key_path() -> PathBuf {
    if let Some(p) = std::env::var_os("VIMORA_RELEASE_KEY") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .expect("no home folder");
    PathBuf::from(home)
        .join(".config")
        .join("vimora-release")
        .join("release.key")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn load() -> SigningKey {
    let p = key_path();
    let text =
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("release key {}: {e}", p.display()));
    let bytes: Vec<u8> = (0..64)
        .map(|i| u8::from_str_radix(&text.trim()[i * 2..i * 2 + 2], 16).expect("bad key file"))
        .collect();
    SigningKey::from_bytes(&bytes[..32].try_into().unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("keygen") => {
            let p = key_path();
            if p.exists() {
                eprintln!("{} already exists; not overwritten", p.display());
                std::process::exit(1);
            }
            let mut seed = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut seed);
            let k = SigningKey::from_bytes(&seed);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            // seed (32 bytes) + public key (32 bytes), hex
            std::fs::write(
                &p,
                format!("{}{}\n", hex(&seed), hex(k.verifying_key().as_bytes())),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
            }
            println!("{}", hex(k.verifying_key().as_bytes()));
        }
        Some("public") => println!("{}", hex(load().verifying_key().as_bytes())),
        Some("sign") => {
            let file = args.get(1).expect("sign <file>");
            let data = std::fs::read(file).expect("cannot read the file");
            let sig = load().sign(&vimora_update::signed_message(&data));
            std::fs::write(format!("{file}.sig"), format!("{}\n", hex(&sig.to_bytes())))
                .expect("cannot write the signature");
            // Check it at once with the keys built into the program.
            vimora_update::verify_sums(&data, &hex(&sig.to_bytes()))
                .expect("the signature does not match the public key built into Vimora");
            println!("{file}.sig");
        }
        _ => {
            eprintln!("usage: sign keygen | public | sign <file>");
            std::process::exit(2);
        }
    }
}
