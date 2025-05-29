use case_insensitive_hashmap::CaseInsensitiveHashMap;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

fn iter_map<P: AsRef<Path> + ?Sized>(path: &P, map: &mut CaseInsensitiveHashMap<OsString>) -> () {
    let files = match std::fs::read_dir(path) {
        Ok(files) => files,
        Err(_) => return,
    };
    for entry in files {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                iter_map(&path, map);
            } else if path.is_file() {
                if let Some(ext) = path.extension().map(|s| s.to_ascii_lowercase()) {
                    if ext == "crx" || ext == "pck" {
                        if let Some(name) = path.file_name() {
                            map.insert(
                                name.to_string_lossy().into_owned(),
                                path.clone().into_os_string(),
                            );
                        }
                    }
                }
            }
        }
    }
}

pub fn gate_base_path() -> PathBuf {
    let p = std::env::current_exe()
        .map(|e| e.parent().map(|p| p.to_path_buf()))
        .unwrap_or(Some(Path::new(".").to_path_buf()))
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    p
}

pub fn get_advdata_map() -> CaseInsensitiveHashMap<OsString> {
    let mut map = CaseInsensitiveHashMap::new();
    let mut p = None;
    let files = match std::fs::read_dir(BASE_PATH.as_path()) {
        Ok(files) => files,
        Err(_) => return map,
    };
    for f in files {
        if let Ok(entry) = f {
            let path = entry.path();
            if path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|f| f.to_ascii_lowercase() == "advdata")
            {
                p = Some(path);
                break;
            }
        }
    }
    let p = match p {
        Some(p) => p,
        None => return map,
    };
    iter_map(&p, &mut map);
    map
}

lazy_static::lazy_static! {
    pub static ref BASE_PATH: PathBuf = gate_base_path();
    pub static ref ADV_DATA_MAP: CaseInsensitiveHashMap<OsString> = get_advdata_map();
}
