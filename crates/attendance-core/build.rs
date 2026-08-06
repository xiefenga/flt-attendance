use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let data_dir = manifest_dir.join("data/holidays");
    println!("cargo:rerun-if-changed={}", data_dir.display());

    let mut calendars = fs::read_dir(&data_dir)
        .expect("holiday data directory should exist")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| {
            let year = path.file_stem()?.to_str()?.parse::<u16>().ok()?;
            Some((year, path))
        })
        .collect::<Vec<_>>();
    calendars.sort_by_key(|(year, _)| *year);

    let mut generated =
        String::from("pub(crate) const EMBEDDED_HOLIDAY_JSON: &[(u16, &str)] = &[\n");
    for (year, path) in calendars {
        generated.push_str(&format!("    ({year}, include_str!({path:?})),\n"));
    }
    generated.push_str("];\n");

    let output =
        PathBuf::from(env::var("OUT_DIR").expect("missing output dir")).join("holiday_data.rs");
    fs::write(output, generated).expect("holiday data source should be generated");
}
