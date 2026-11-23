//! Comprime el api-dump.json de rustudio (7 MB → ~0,6 MB) para llevarlo
//! dentro del ejecutable; la pestaña Crear lo descomprime la primera vez.

use std::io::Write;

fn main() {
    let origen = std::path::Path::new("rustudio/api-dump.json");
    println!("cargo:rerun-if-changed={}", origen.display());
    let datos = std::fs::read(origen).expect("rustudio/api-dump.json");
    let salida = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("api-dump.json.gz");
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    gz.write_all(&datos).unwrap();
    std::fs::write(salida, gz.finish().unwrap()).unwrap();
}
