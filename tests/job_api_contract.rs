use std::sync::{Mutex, OnceLock};
use vim_core_rs::{CoreCommandError, CoreHostAction, JobStatus, VimCoreSession};

fn session_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn acquire_session_test_lock() -> std::sync::MutexGuard<'static, ()> {
    session_test_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn test_inject_vfd_data_and_notify_status() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").unwrap();

    session
        .apply_ex_command("call job_start(['echo', 'hello'])")
        .unwrap();

    let mut job_id = None;
    let mut vfd_out = None;

    while let Some(action) = session.take_pending_host_action() {
        if let CoreHostAction::JobStart(req) = action {
            job_id = Some(req.job_id);
            vfd_out = Some(req.vfd_out);
        }
    }

    let job_id = job_id.expect("JobStart should be emitted");
    let vfd_out = vfd_out.expect("vfd_out should be populated");

    let res1 = session.inject_vfd_data(vfd_out, b"world\n");
    assert_eq!(res1, Ok(()));

    let res2 = session.notify_job_status(job_id, JobStatus::Finished, 0);
    assert_eq!(res2, Ok(()));
}

#[test]
fn test_job_rejection_and_status() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").unwrap();

    session
        .apply_ex_command("let g:my_job = job_start(['echo', 'hello'])")
        .unwrap();

    let mut job_id = None;
    while let Some(action) = session.take_pending_host_action() {
        if let CoreHostAction::JobStart(req) = action {
            job_id = Some(req.job_id);
        }
    }
    let job_id = job_id.unwrap();

    session
        .notify_job_status(job_id, JobStatus::Failed, -1)
        .unwrap();

    session
        .apply_ex_command("call append(0, job_status(g:my_job))")
        .unwrap();
    session
        .apply_ex_command("call append(1, job_info(g:my_job).exitval)")
        .unwrap();

    let buf_id = session
        .snapshot()
        .windows
        .iter()
        .find(|w| w.is_active)
        .unwrap()
        .buf_id;
    let buf_text = session.buffer_text(buf_id).unwrap();
    let lines: Vec<&str> = buf_text.lines().collect();

    assert_eq!(lines[0], "dead");
    assert_eq!(lines[1], "-1");
}

#[test]
fn test_session_cleanup_on_drop() {
    let _guard = acquire_session_test_lock();
    let mut job_id = None;
    let mut vfd_out = None;

    {
        let mut session = VimCoreSession::new("").unwrap();
        session
            .apply_ex_command("call job_start(['echo', 'hello'])")
            .unwrap();

        while let Some(action) = session.take_pending_host_action() {
            if let CoreHostAction::JobStart(req) = action {
                job_id = Some(req.job_id);
                vfd_out = Some(req.vfd_out);
            }
        }
    }

    let job_id = job_id.unwrap();
    let vfd_out = vfd_out.unwrap();

    let mut session = VimCoreSession::new("").unwrap();

    let res = session.inject_vfd_data(vfd_out, b"world\n");
    assert_eq!(res, Err(CoreCommandError::InvalidInput));

    let res2 = session.notify_job_status(job_id, JobStatus::Finished, 0);
    assert_eq!(res2, Err(CoreCommandError::InvalidInput));
}

#[test]
fn test_event_loop_interference() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("initial").unwrap();

    session
        .apply_ex_command("let g:my_job = job_start(['sleep', '10'])")
        .unwrap();

    // Verify Vim is not blocked and can still process UI events / commands
    session
        .apply_ex_command("call append(0, 'still alive')")
        .unwrap();
    let buf_id = session
        .snapshot()
        .windows
        .iter()
        .find(|w| w.is_active)
        .unwrap()
        .buf_id;
    let buf_text = session.buffer_text(buf_id).unwrap();
    assert!(buf_text.contains("still alive"));

    let mut job_id = None;
    while let Some(action) = session.take_pending_host_action() {
        if let CoreHostAction::JobStart(req) = action {
            job_id = Some(req.job_id);
        }
    }

    // Complete the job
    if let Some(id) = job_id {
        session
            .notify_job_status(id, JobStatus::Finished, 0)
            .unwrap();
    }
}

#[test]
fn test_in_memory_host_e2e_normal() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").unwrap();

    session
        .apply_ex_command("let g:my_job = job_start(['echo', 'hello'])")
        .unwrap();

    let mut job_id = None;
    let mut vfd_out = None;

    while let Some(action) = session.take_pending_host_action() {
        if let CoreHostAction::JobStart(req) = action {
            job_id = Some(req.job_id);
            vfd_out = Some(req.vfd_out);
        }
    }

    let job_id = job_id.unwrap();
    let vfd_out = vfd_out.unwrap();

    // Inject "hello" to the output VFD
    session
        .inject_vfd_data(vfd_out, b"hello from rust\n")
        .unwrap();

    session
        .apply_ex_command("call append(0, 'JOB: ' . job_status(g:my_job))")
        .unwrap();
    session
        .apply_ex_command("call append(1, 'CHAN: ' . ch_status(g:my_job))")
        .unwrap();

    // Synchronously read from the channel in Vim script
    let read_result =
        session.apply_ex_command("let g:job_out = ch_readraw(g:my_job, {'part': 'out'})");
    if let Err(e) = &read_result {
        println!("READ ERR: {:?}", e);
    }

    // Finish the job
    session
        .notify_job_status(job_id, JobStatus::Finished, 0)
        .unwrap();

    // Check variable content.
    session
        .apply_ex_command("call append(0, g:job_out)")
        .unwrap();

    let buf_id = session
        .snapshot()
        .windows
        .iter()
        .find(|w| w.is_active)
        .unwrap()
        .buf_id;
    let buf_text = session.buffer_text(buf_id).unwrap();
    println!("BUFFER TEXT: {}", buf_text);
    let lines: Vec<&str> = buf_text.lines().collect();

    assert!(lines.contains(&"hello from rust"));
}

#[test]
fn test_job_communication_mismatch() {
    let _guard = acquire_session_test_lock();
    let mut session = VimCoreSession::new("").unwrap();

    session
        .apply_ex_command("let g:my_job = job_start(['echo', 'hello'])")
        .unwrap();

    let mut job_id = None;
    let mut vfd_out = None;

    while let Some(action) = session.take_pending_host_action() {
        if let CoreHostAction::JobStart(req) = action {
            job_id = Some(req.job_id);
            vfd_out = Some(req.vfd_out);
        }
    }

    let job_id = job_id.unwrap();
    let vfd_out = vfd_out.unwrap();

    // Kill the job immediately
    session
        .notify_job_status(job_id, JobStatus::Failed, -1)
        .unwrap();

    // Further communication should fail
    let res = session.inject_vfd_data(vfd_out, b"late data\n");
    // Depending on the exact logic, it might return Ok if VFD is not strictly dropped, but it's closed,
    // so inject_data returns false.
    assert_eq!(res, Err(CoreCommandError::InvalidInput));
}
