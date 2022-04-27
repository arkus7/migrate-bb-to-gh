use std::error::Error;
use std::fs::write;

fn main() -> Result<(), Box<dyn Error>> {
    let config = include_bytes!("config.yml");
    let encoded = base64::encode(config);
    write("config.encrypted.yml", encoded)?;
    println!("cargo:rerun-if-changed=config.yml");
    Ok(())
}
