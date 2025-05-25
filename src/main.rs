pub mod args;
pub mod crx;
pub mod ext;
pub mod utils;

pub fn auto(input: &str) -> anyhow::Result<()> {
    let pb = std::path::PathBuf::from(input);
    let ext = pb.extension().unwrap_or(std::ffi::OsStr::new(""));
    if ext.to_ascii_lowercase() == "crx" {
        let crx = crx::Crx::read_from_file(&pb)?;
        let mut pb2 = pb.clone();
        let mut failed = false;
        let mut removed = Vec::new();
        while pb2
            .file_name()
            .is_some_and(|f| f.to_ascii_lowercase() != "advdata")
        {
            pb2.file_name().map(|s| removed.push(s.to_owned()));
            if !pb2.pop() {
                failed = true;
                eprintln!(
                    "Failed to find 'advdata' directory in path: {}",
                    pb.display()
                );
                break;
            }
        }
        if !failed {
            pb2.file_name().map(|s| removed.push(s.to_owned()));
            pb2.pop();
        }
        let output_path = if failed {
            pb2.with_extension("png")
        } else {
            let mut p = pb2.join("extracted");
            loop {
                match removed.pop() {
                    Some(name) => p.push(name),
                    None => break,
                }
            }
            p.with_extension("png")
        };
        utils::make_sure_dir_exists(&output_path)?;
        crx.export_png(&output_path)?;
    }
    Ok(())
}

fn main() {
    let args = args::Arg::parse();
    unsafe { std::env::set_var("RUST_LIB_BACKTRACE", "1") };
    if let Some(arg) = args.auto.as_ref() {
        let e = match auto(&arg.input) {
            Ok(_) => {
                eprintln!("Auto operation completed successfully.");
                false
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!("Backtrace: {}", e.backtrace());
                true
            }
        };
        if e {
            eprintln!("Press Enter to exit program.");
            let mut s = String::new();
            let _ = std::io::stdin().read_line(&mut s);
        }
    }
}
