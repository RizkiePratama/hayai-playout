use anyhow::{anyhow, Result};
use gstreamer as gst;
use gst::prelude::*;
use std::sync::{Arc, Mutex};

use super::models::PlaylistItem;

pub(crate) fn handle_eos(
    p: &gst::Pipeline,
    vs: &gst::Element,
    as_: &gst::Element,
    pl: &Arc<Mutex<Vec<PlaylistItem>>>,
    pid: &Arc<Mutex<Option<u64>>>,
    finished_src: gst::Element,
) {
    println!("[DEBUG] EOS received from: {}", finished_src.name());
    let p_clone = p.clone();
    let vs_clone = vs.clone();
    let as_clone = as_.clone();
    let pl_clone = pl.clone();
    let pid_clone = pid.clone();
    // Spawning a thread is still good practice to avoid blocking the GStreamer bus thread.
    std::thread::spawn(move || {
        if let Err(e) = play_next(&p_clone, &vs_clone, &as_clone, &pl_clone, &pid_clone, Some(finished_src)) {
            eprintln!("[hayai] Failed to handle transition: {}", e);
        }
    });
}

pub(crate) fn play_next(
    p: &gst::Pipeline,
    vs: &gst::Element,
    as_: &gst::Element,
    pl_arc: &Arc<Mutex<Vec<PlaylistItem>>>,
    pid_arc: &Arc<Mutex<Option<u64>>>,
    element_to_remove: Option<gst::Element>,
) -> Result<()> {
    println!("[DEBUG] play_next: Starting transition.");
    let playlist = pl_arc.lock().unwrap();
    let mut playing_id = pid_arc.lock().unwrap();

    // --- LOGGING: Print the current state of the playlist ---
    println!("[DEBUG] play_next: Current playlist state: {:?}", playlist);
    println!("[DEBUG] play_next: Currently playing ID: {:?}", *playing_id);

    if playlist.is_empty() { 
        println!("[ERROR] play_next: Playlist is empty, cannot play next item.");
        return Err(anyhow!("Playlist is empty")); 
    }

    let mut next_index = 0;
    if let Some(id) = *playing_id {
        if let Some(current_index) = playlist.iter().position(|item| item.id == id) {
            next_index = (current_index + 1) % playlist.len();
        }
    }

    let next_item = playlist[next_index].clone();
    let new_id = next_item.id;
    println!("[DEBUG] play_next: Next item to play: (index {}) {}", next_index, next_item.uri);
    drop(playlist); // Release lock before calling switch_source

    switch_source(p, vs, as_, &next_item, element_to_remove)?;
    *playing_id = Some(new_id);
    println!("[DEBUG] play_next: Transition complete. New playing ID: {:?}", *playing_id);
    Ok(())
}

fn switch_source(
    pipeline: &gst::Pipeline,
    v_selector: &gst::Element,
    a_selector: &gst::Element,
    item: &PlaylistItem,
    old_source: Option<gst::Element>,
) -> Result<()> {
    println!("[DEBUG] switch_source: Creating new source for: {}", item.uri);
    let source_elem = gst::ElementFactory::make("uridecodebin")
        .name(&format!("source_elem_{}", item.id))
        .build()?;
    source_elem.set_property("uri", &item.uri);

    // Add the new element to the pipeline first.
    pipeline.add(&source_elem)?;
    
    let v_selector_clone = v_selector.clone();
    let a_selector_clone = a_selector.clone();
    
    source_elem.connect_pad_added(move |_, pad| {
        println!("[DEBUG] pad-added: Fired for pad '{}'", pad.name());
        if let Some(caps) = pad.current_caps() {
            if let Some(s) = caps.structure(0) {
                let media_type = s.name();
                println!("[DEBUG] pad-added: Media type is '{}'", media_type);
                if media_type.starts_with("video/") {
                    let sink_pad = v_selector_clone.request_pad_simple("sink_%u").unwrap();
                    println!("[DEBUG] pad-added: Linking video pad to selector pad '{}'", sink_pad.name());
                    if let Err(e) = pad.link(&sink_pad) { eprintln!("[hayai] Failed to link video pad: {}", e); } 
                    else { v_selector_clone.set_property("active-pad", &sink_pad); }
                } else if media_type.starts_with("audio/") {
                    let sink_pad = a_selector_clone.request_pad_simple("sink_%u").unwrap();
                    println!("[DEBUG] pad-added: Linking audio pad to selector pad '{}'", sink_pad.name());
                    if let Err(e) = pad.link(&sink_pad) { eprintln!("[hayai] Failed to link audio pad: {}", e); }
                    else { a_selector_clone.set_property("active-pad", &sink_pad); }
                }
            }
        }
    });

    // Now, schedule the cleanup of the OLD source to happen asynchronously.
    if let Some(old_elem) = old_source {
        println!("[DEBUG] switch_source: Scheduling cleanup for old element: {}", old_elem.name());
        let pipeline_clone = pipeline.clone();
        let v_selector_clone = v_selector.clone();
        let a_selector_clone = a_selector.clone();
        
        pipeline.call_async(move |_| {
            println!("[DEBUG] call_async: Now cleaning up old element '{}'", old_elem.name());
            
            // Set state to NULL to release resources.
            let _ = old_elem.set_state(gst::State::Null);
            
            // Release the selector pads that were connected to the old element.
            let release_pads = |selector: &gst::Element, element_to_remove: &gst::Element| {
                for pad in selector.sink_pads() {
                    if let Some(peer) = pad.peer() {
                        if peer.parent_element().as_ref() == Some(element_to_remove) {
                            println!("[DEBUG] call_async: Releasing selector pad '{}'", pad.name());
                            selector.release_request_pad(&pad);
                        }
                    }
                }
            };
            release_pads(&v_selector_clone, &old_elem);
            release_pads(&a_selector_clone, &old_elem);
            
            // Finally, remove the old element from the pipeline.
            println!("[DEBUG] call_async: Removing old element from pipeline.");
            let _ = pipeline_clone.remove(&old_elem);
        });
    }
    
    // Tell the new source to start playing.
    source_elem.sync_state_with_parent()?;
    println!("[DEBUG] switch_source: New source '{}' is now synchronized.", item.uri);
    Ok(())
}