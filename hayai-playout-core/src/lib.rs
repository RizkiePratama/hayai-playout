use anyhow::{anyhow, Result};
use gstreamer as gst;
use gst::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistItem {
    pub id: u64,
    pub uri: String,
}

pub struct Streamer {
    pipeline: Option<gst::Pipeline>,
    input_selector: Option<gst::Element>,
    playlist: Arc<Mutex<Vec<PlaylistItem>>>,
    currently_playing_id: Arc<Mutex<Option<u64>>>,
    main_loop: Option<glib::MainLoop>,
    main_loop_thread: Option<std::thread::JoinHandle<()>>,
}

impl Streamer {
    pub fn new() -> Result<Self> {
        gst::init()?;
        Ok(Self {
            pipeline: None,
            input_selector: None,
            playlist: Arc::new(Mutex::new(Vec::new())),
            currently_playing_id: Arc::new(Mutex::new(None)),
            main_loop: None,
            main_loop_thread: None,
        })
    }

    pub fn add_item(&self, uri: &str) {
        let mut playlist = self.playlist.lock().unwrap();
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        playlist.push(PlaylistItem {
            id,
            uri: uri.to_string(),
        });
    }

    pub fn remove_item(&self, id: u64) {
        let mut playlist = self.playlist.lock().unwrap();
        playlist.retain(|item| item.id != id);
    }

    pub fn move_item(&self, id: u64, new_index: usize) -> Result<()> {
        let mut playlist = self.playlist.lock().unwrap();
        if new_index >= playlist.len() {
            return Err(anyhow!("New index is out of bounds"));
        }
        let old_index = playlist.iter().position(|item| item.id == id)
            .ok_or_else(|| anyhow!("Item ID not found in playlist"))?;
        
        let item = playlist.remove(old_index);
        playlist.insert(new_index, item);
        Ok(())
    }

    pub fn get_playlist_clone(&self) -> Vec<PlaylistItem> {
        self.playlist.lock().unwrap().clone()
    }

    pub fn get_currently_playing_id(&self) -> Option<u64> {
        *self.currently_playing_id.lock().unwrap()
    }

    pub fn start(&mut self, rtmp_url: &str) -> Result<()> {
        if self.pipeline.is_some() {
            return Err(anyhow!("Stream is already running"));
        }

        let pipeline = gst::Pipeline::new();
        let selector = gst::ElementFactory::make("input-selector").build()?;
        let processing_bin = self.create_processing_bin(rtmp_url)?;

        pipeline.add_many(&[
            selector.upcast_ref::<gst::Element>(),
            processing_bin.upcast_ref::<gst::Element>(),
        ])?;
        selector.link(&processing_bin)?;
        
        let bus = pipeline.bus().unwrap();
        
        let weak_pipeline = pipeline.downgrade();
        let weak_selector = selector.downgrade();
        let playlist_clone = self.playlist.clone();
        let playing_id_clone = self.currently_playing_id.clone();

        bus.set_sync_handler(move |_, msg| {
            if let (Some(pipeline), Some(selector)) = (weak_pipeline.upgrade(), weak_selector.upgrade()) {
                if let gst::MessageView::Eos(m) = msg.view() {
                    if m.src().map_or(false, |s| s.name().starts_with("source_bin_")) {
                        Self::handle_eos(&pipeline, &selector, &playlist_clone, &playing_id_clone);
                    }
                }
            }
            gst::BusSyncReply::Drop
        });
        
        self.pipeline = Some(pipeline);
        self.input_selector = Some(selector);
        
        Self::play_next(self.pipeline.as_ref().unwrap(), self.input_selector.as_ref().unwrap(), &self.playlist, &self.currently_playing_id)?;
        
        self.pipeline.as_ref().unwrap().set_state(gst::State::Playing)?;

        let main_loop = glib::MainLoop::new(None, false);
        let loop_handle = std::thread::spawn({
            let main_loop = main_loop.clone();
            move || {
                main_loop.run();
            }
        });
        self.main_loop = Some(main_loop);
        self.main_loop_thread = Some(loop_handle);

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(main_loop) = self.main_loop.take() {
            main_loop.quit();
        }
        if let Some(thread_handle) = self.main_loop_thread.take() {
            let _ = thread_handle.join();
        }
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.set_state(gst::State::Null)?;
        }
        self.input_selector = None;
        *self.currently_playing_id.lock().unwrap() = None;
        Ok(())
    }

    fn create_source_bin(item: &PlaylistItem) -> Result<gst::Bin> {
        let bin = gst::Bin::with_name(&format!("source_bin_{}", item.id));
        let src = gst::ElementFactory::make("filesrc").build()?;
        let decode = gst::ElementFactory::make("decodebin").build()?;
        
        src.set_property("location", &item.uri);

        bin.add_many(&[&src, &decode])?;
        gst::Element::link_many(&[&src, &decode])?;

        let decode_clone = decode.clone();
        decode.connect_pad_added(move |_, src_pad| {
            let ghost_pad_name = format!("src_{}", src_pad.name());
            let ghost_pad = gst::GhostPad::with_target(src_pad).unwrap();
            
            // FIX: Use the underlying property system, which is more robust.
            ghost_pad.set_property("name", &ghost_pad_name);

            let _ = ghost_pad.set_active(true);
            let _ = decode_clone.parent().unwrap().downcast::<gst::Bin>().unwrap().add_pad(&ghost_pad);
        });

        Ok(bin)
    }

    fn create_processing_bin(&self, rtmp_url: &str) -> Result<gst::Bin> {
        let bin = gst::Bin::with_name("processing_bin");
        let queue = gst::ElementFactory::make("queue").build()?;
        let vconv = gst::ElementFactory::make("videoconvert").build()?;
        let venc = gst::ElementFactory::make("x264enc").build()?;
        let aconv = gst::ElementFactory::make("audioconvert").build()?;
        let aenc = gst::ElementFactory::make("voaacenc").build()?;
        let mux = gst::ElementFactory::make("flvmux").name("mux").build()?;
        let sink = gst::ElementFactory::make("rtmpsink").build()?;
        
        venc.set_property_from_str("tune", "zerolatency");
        sink.set_property("location", rtmp_url);
        
        bin.add_many(&[&queue, &vconv, &venc, &aconv, &aenc, &mux, &sink])?;
        gst::Element::link_many(&[&queue, &vconv, &venc])?;
        gst::Element::link_many(&[&aconv, &aenc])?;
        venc.link(&mux)?;
        aenc.link(&mux)?;
        mux.link(&sink)?;

        let target_pad = queue.static_pad("sink").unwrap();
        let sink_pad = gst::GhostPad::with_target(&target_pad)?;
        sink_pad.set_property("name", "sink");
        bin.add_pad(&sink_pad)?;
        Ok(bin)
    }
    
    fn handle_eos( pipeline: &gst::Pipeline, selector: &gst::Element, playlist: &Arc<Mutex<Vec<PlaylistItem>>>, playing_id: &Arc<Mutex<Option<u64>>>) {
        if let Err(e) = Self::play_next(pipeline, selector, playlist, playing_id) {
            eprintln!("Failed to play next item: {}", e);
        }
    }

    fn play_next( pipeline: &gst::Pipeline, selector: &gst::Element, playlist_arc: &Arc<Mutex<Vec<PlaylistItem>>>, playing_id_arc: &Arc<Mutex<Option<u64>>> ) -> Result<()> {
        let playlist = playlist_arc.lock().unwrap();
        let mut playing_id = playing_id_arc.lock().unwrap();

        if playlist.is_empty() {
            println!("Playlist is empty. Nothing to play.");
            return Ok(());
        }

        let mut next_index = 0;
        if let Some(id) = *playing_id {
            if let Some(current_index) = playlist.iter().position(|item| item.id == id) {
                next_index = (current_index + 1) % playlist.len();
            }
        }
        
        let next_item = playlist[next_index].clone();
        let new_id = next_item.id;
        
        drop(playlist);
        
        Self::switch_source(pipeline, selector, &next_item)?;

        *playing_id = Some(new_id);
        
        Ok(())
    }

    fn switch_source(pipeline: &gst::Pipeline, selector: &gst::Element, item: &PlaylistItem) -> Result<()> {
        let source_bin = Self::create_source_bin(item)?;
        pipeline.add(&source_bin)?;
        
        let selector_clone = selector.clone();
        source_bin.connect_pad_added(move |_, pad| {
            let selector_sink_pad = selector_clone.request_pad_simple("sink_%u").unwrap();
            pad.link(&selector_sink_pad).unwrap();
        });

        let pipeline_clone = pipeline.clone();
        let selector_clone = selector.clone();
        pipeline.call_async(move |_| {
            if let Some(active_pad) = selector_clone.property::<Option<gst::Pad>>("active-pad") {
                if let Some(peer) = active_pad.peer() {
                    if let Some(parent_bin) = peer.parent_element() {
                        let _ = parent_bin.set_state(gst::State::Null);
                        let _ = pipeline_clone.remove(&parent_bin);
                    }
                }
            }
            if let Some(last_pad) = selector_clone.pads().last() {
                selector_clone.set_property("active-pad", last_pad);
            }
        });
        
        source_bin.sync_state_with_parent()?;
        Ok(())
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        if self.pipeline.is_some() {
            let _ = self.stop();
        }
    }
}