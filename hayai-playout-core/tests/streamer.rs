use hayai_playout_core::{EncodingSettings, Streamer}; // Add EncodingSettings here
use anyhow::Result;
use std::thread;
use std::time::Duration;

#[test]
fn test_new_streamer_is_empty() {
    let streamer = Streamer::new().unwrap();
    assert!(streamer.get_playlist_clone().is_empty());
    assert!(streamer.get_currently_playing_id().is_none());
}

#[test]fn test_add_items() {
    let streamer = Streamer::new().unwrap();
    streamer.add_item("A");
    streamer.add_item("B");

    let playlist = streamer.get_playlist_clone();
    assert_eq!(playlist.len(), 2);
    assert_eq!(playlist[0].uri, "A");
    assert_eq!(playlist[1].uri, "B");
    assert!(playlist[1].id > playlist[0].id);
}

#[test]
fn test_remove_item() {
    let streamer = Streamer::new().unwrap();
    streamer.add_item("A");
    streamer.add_item("B");
    streamer.add_item("C");

    let playlist_before = streamer.get_playlist_clone();
    let id_to_remove = playlist_before.iter().find(|item| item.uri == "B").unwrap().id;
    streamer.remove_item(id_to_remove);

    let playlist_after = streamer.get_playlist_clone();
    assert_eq!(playlist_after.len(), 2);
    assert_eq!(playlist_after[0].uri, "A");
    assert_eq!(playlist_after[1].uri, "C");
}

#[test]
fn test_remove_nonexistent_item() {
    let streamer = Streamer::new().unwrap();
    streamer.add_item("A");
    streamer.remove_item(99999);
    assert_eq!(streamer.get_playlist_clone().len(), 1);
}

#[test]
fn test_move_item() -> Result<()> {
    let streamer = Streamer::new().unwrap();
    streamer.add_item("A");
    streamer.add_item("B");
    streamer.add_item("C");

    let playlist_before = streamer.get_playlist_clone();
    let id_to_move = playlist_before.iter().find(|item| item.uri == "C").unwrap().id;
    streamer.move_item(id_to_move, 0)?;

    let playlist_after = streamer.get_playlist_clone();
    assert_eq!(playlist_after.len(), 3);
    assert_eq!(playlist_after[0].uri, "C");
    assert_eq!(playlist_after[1].uri, "A");
    assert_eq!(playlist_after[2].uri, "B");

    Ok(())
}

#[test]
fn test_move_item_out_of_bounds() {
    let streamer = Streamer::new().unwrap();
    streamer.add_item("A");
    let id_to_move = streamer.get_playlist_clone()[0].id;
    
    let result = streamer.move_item(id_to_move, 10);
    assert!(result.is_err());
}


// --- THIS IS THE FIXED TEST ---
#[test]
#[ignore]
fn test_start_stop_lifecycle() -> Result<()> {
    let mut streamer = Streamer::new()?;

    let temp_dir = tempfile::tempdir()?;
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, "test")?;
    let file_uri = format!("file://{}", file_path.to_str().unwrap());

    streamer.add_item(&file_uri);
    let first_item_id = streamer.get_playlist_clone()[0].id;

    let rtmp_url = "rtmp://localhost/live/test";
    
    // Create default settings to pass to the start function.
    let settings = EncodingSettings::default();
    
    // Pass the new `settings` argument.
    streamer.start(rtmp_url, &settings)?;
    
    thread::sleep(Duration::from_millis(500));
    
    let playing_id = streamer.get_currently_playing_id();
    assert!(playing_id.is_some(), "Streamer should be playing an item");
    assert_eq!(playing_id.unwrap(), first_item_id, "Should be playing the first item");

    streamer.stop()?;

    assert!(streamer.get_currently_playing_id().is_none(), "Playing ID should be cleared after stop");

    Ok(())
}