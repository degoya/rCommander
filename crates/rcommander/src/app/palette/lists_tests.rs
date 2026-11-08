use super::*;

#[test]
fn a_chosen_file_runs_its_command() {
    let path = PathBuf::from("/tmp/mark.txt");
    assert_eq!(
        command(ListDialog::SelectionToFile, path.clone()),
        Some(Command::SelectionToFile {
            panel: None,
            path: "/tmp/mark.txt".into(),
            overwrite: true,
        })
    );
    assert_eq!(
        command(ListDialog::LoadList, path.clone()),
        Some(Command::LoadList {
            panel: None,
            path: "/tmp/mark.txt".into(),
        })
    );
    assert_eq!(command(ListDialog::SelectionFromClipboard, path), None);
}
