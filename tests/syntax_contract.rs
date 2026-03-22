use vim_core_rs::{VimCoreSession};

#[test]
fn test_syntax_highlight_extraction() {
    let mut session = VimCoreSession::new("").expect("session should initialize");
    
    // Get the window ID
    let windows = session.windows();
    assert!(!windows.is_empty());
    let win_id = windows[0].id;
    let buf_id = windows[0].buf_id;

    // Enable syntax manually without runtime files
    session.apply_ex_command(":syntax on").ok(); // may fail if no runtime, that's fine
    
    // Create a syntax rule: match "Error" for the word "hello"
    session.apply_ex_command(":syntax match Error /hello/").expect("syntax match should succeed");
    
    // Insert text
    session.apply_normal_command("ihello world\x1b").expect("insert should succeed");
    assert_eq!(session.buffer_text(buf_id).unwrap(), "hello world");
    
    // Get line syntax for line 1
    let chunks = session.get_line_syntax(win_id, 1).expect("should get syntax");
    
    // "hello" is 5 chars. Space is 1. "world" is 5.
    // Syntax chunks should represent these.
    // Since we matched "hello" with Error, the first 5 columns should have name "Error" or something similar.
    
    assert!(!chunks.is_empty(), "Chunks should not be empty");
    
    let hello_chunk = &chunks[0];
    assert_eq!(hello_chunk.start_col, 0);
    assert_eq!(hello_chunk.end_col, 5);
    assert_eq!(hello_chunk.name.as_deref(), Some("Error"));
    
    if chunks.len() > 1 {
        let rest_chunk = &chunks[1];
        assert_eq!(rest_chunk.start_col, 5);
        // The remaining chunk should have a different name or None
        assert_ne!(rest_chunk.name.as_deref(), Some("Error"));
    }
}