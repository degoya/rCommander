use std::path::Path;

use rc_vfs::MemoryFs;

use super::*;

#[tokio::test]
async fn a_temporary_name_is_free_and_beside_the_entry() {
    let fs = MemoryFs::new();
    let temp = free_temp_name(&fs, Path::new("/dir/a.txt"))
        .await
        .expect("a free name");
    assert_eq!(temp.parent(), Some(Path::new("/dir")));
    let name = temp.file_name().expect("a name").to_string_lossy();
    assert!(name.starts_with(".rc-tmp-"), "{name}");
}
