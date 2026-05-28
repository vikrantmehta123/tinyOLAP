use std::path::Path;
use std::io;

pub fn discover_parts(table_dir: &Path) -> io::Result<Vec<u32>> {
    let mut part_ids = Vec::new();
    for entry in std::fs::read_dir(table_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(suffix) = name.strip_prefix("part_") {
            if let Ok(id) = suffix.parse::<u32>() {
                part_ids.push(id);
            }
        }
    }
    part_ids.sort();
    Ok(part_ids)
}
