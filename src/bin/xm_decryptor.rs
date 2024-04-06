use std::path::PathBuf;

use clap::Parser;

use xm_decryptor::{xm, Result};

/// Ximalaya xm file decryptor
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Dry run?
    #[arg(short = 'n', long, default_value_t = false)]
    dry_run: bool,

    #[clap(index = 1)]
    pub xm_file: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    // let path = PathBuf::from(std::env::args().nth(1).expect("no input path"));
    let path = args.xm_file;
    let mut files = Vec::<PathBuf>::new();
    if path.is_file() {
        files.push(path);
    } else if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            }
        }
    }
    let files: Vec<_> = files
        .into_iter()
        .filter(|f| f.extension().unwrap_or_default() == "xm")
        .collect();
    for file in files {
        if let Err(e) = decrypt_file(&file, args.dry_run) {
            eprintln!("error: {:?} {:?}", file, e);
        }
    }
    Ok(())
}

fn decrypt_file(file: &PathBuf, dry_run: bool) -> Result<()> {
    let content = std::fs::read(file)?;

    let xm_info = xm::extract_xm_info(&content[..])?;
    println!("xm_info: {:?}", xm_info);

    let audio = xm::decrypt(&xm_info, &content[..])?;
    let file_name = xm_info.file_name(&audio[..0xFF], file.file_stem().unwrap().to_str().unwrap());

    let target_path = file.parent().expect("no parent dir").join(file_name);
    println!("target_path: {:?}", target_path);
    if !dry_run {
        std::fs::write(target_path, audio)?;
    }
    Ok(())
}
