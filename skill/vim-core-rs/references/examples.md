# vim-core-rs usage examples

These examples show how to use the crate as a host. Adapt them to the user task
instead of copying them blindly.

## Handle the host-action loop

```rust
use vim_core_rs::{VimCoreSession, CoreHostAction};

fn main() {
    let mut session = VimCoreSession::new("Initial text").unwrap();
    
    session.apply_ex_command("set number").unwrap();
    
    // Process pending actions from the core engine
    while let Some(action) = session.take_pending_host_action() {
        match action {
            CoreHostAction::VfsRequest(req) => {
                // Handle Virtual File System request (resolve, load, save)
            }
            CoreHostAction::Quit { force, .. } => {
                // Exit application
                break;
            }
            CoreHostAction::Redraw { .. } => {
                // Trigger UI refresh
            }
            _ => {}
        }
    }
}
```

In real hosts, replace the comments with actual VFS, redraw, quit, and job
handling. The loop is not optional for commands that queue host work.

## Inspect a coherent snapshot

```rust
use vim_core_rs::{VimCoreSession, CoreMode};

fn check_mode() {
    let session = VimCoreSession::new("Hello").unwrap();
    let snapshot = session.snapshot();

    assert_eq!(snapshot.mode, CoreMode::Normal);
    assert_eq!(snapshot.text, "Hello");
}
```

## Capture Vim messages

```rust
use std::sync::{Arc, Mutex};
use vim_core_rs::{CoreMessageEvent, CoreMessageKind, VimCoreSession};

fn capture_messages() {
    let mut session = VimCoreSession::new("hello").unwrap();
    let events: Arc<Mutex<Vec<CoreMessageEvent>>> =
        Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();

    session.set_message_handler(Box::new(move |event| {
        sink.lock().unwrap().push(event);
    }));

    let _ = session.apply_ex_command("echomsg 'hello from vim'");

    assert!(events.lock().unwrap().iter().any(|event| {
        event.kind == CoreMessageKind::Normal
            && event.content.contains("hello from vim")
    }));
}
```

## Bridge a job through VFD

```rust
use vim_core_rs::{CoreHostAction, JobStatus, VimCoreSession};

fn bridge_job_output() {
    let mut session = VimCoreSession::new("").unwrap();
    session
        .apply_ex_command("let g:job = job_start(['echo', 'hello'])")
        .unwrap();

    while let Some(action) = session.take_pending_host_action() {
        if let CoreHostAction::JobStart(req) = action {
            session.inject_vfd_data(req.vfd_out, b"hello from host\n").unwrap();
            session
                .notify_job_status(req.job_id, JobStatus::Finished, 0)
                .unwrap();
        }
    }
}
```
