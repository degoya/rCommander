//! RC-110-02: an SSH shell back at its prompt reads no panel - its commands ran on the server,
//! not in this machine's folders.

use super::tests::{harness, open_ssh, opened};

#[tokio::test]
async fn an_ssh_prompt_reads_no_panel() {
    let harness = harness().await;
    harness.engine.start_watching();
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    assert!(!harness.engine.shell_returned(info.terminal).await);
}
