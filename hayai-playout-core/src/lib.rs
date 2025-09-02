use anyhow::{anyhow, Result};
use gstreamer as gst;
use gst::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistItem { 
    pub id: u64, 
    pub uri: String 
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodingSettings {
    pub video_encoder: String,
    pub audio_encoder: String,
    pub bitrate_kbps: u32,
    pub speed_preset: String,
    pub scale_enabled: bool,
    pub scale_width: u32,
    pub scale_height: u32,
}

impl Default for EncodingSettings {
    fn default() -> Self {
        Self {
            video_encoder: "x264enc".to_string(),
            audio_encoder: "faac".to_string(),
            bitrate_kbps: 4000,
            speed_preset: "ultrafast".to_string(),
            scale_enabled: false,
            scale_width: 1920,
            scale_height: 1080,
        }
    }
}

pub struct Streamer {
    pipeline: Option<gst::Pipeline>,
    playlist: Arc<Mutex<Vec<PlaylistItem>>>,
    currently_playing_id: Arc<Mutex<Option<u64>>>,
}

impl Streamer {
    pub fn new() -> Result<Self> {
        gst::init()?;
        Ok(Self {
            pipeline: None,
            playlist: Arc::new(Mutex::new(Vec::new())),
            currently_playing_id: Arc::new(Mutex::new(None)),
        })
    }

    pub fn start(&mut self, rtmp_url: &str, settings: &EncodingSettings) -> Result<()> {
        if self.pipeline.is_some() { 
            return Err(anyhow!("Stream is already running")); 
        }

        let pipeline = gst::Pipeline::new();
        
        // Create selectors for switching between sources
        let video_selector = gst::ElementFactory::make("input-selector")
            .name("video_selector")
            .build()?;
        let audio_selector = gst::ElementFactory::make("input-selector")
            .name("audio_selector")
            .build()?;
            
        // Create processing bin
        let processing_bin = create_processing_bin(rtmp_url, settings)?;
        
        // Add elements to pipeline
        pipeline.add_many(&[&video_selector, &audio_selector, processing_bin.upcast_ref()])?;
        
        // Link selectors to processing bin
        video_selector.link_pads(Some("src"), &processing_bin, Some("video_sink"))?;
        audio_selector.link_pads(Some("src"), &processing_bin, Some("audio_sink"))?;
        
        let bus = pipeline.bus().unwrap();
        let weak_pipeline = pipeline.downgrade();
        let playlist_clone = self.playlist.clone();
        let playing_id_clone = self.currently_playing_id.clone();

        // Start a background thread to handle bus messages
        let bus_clone = bus.clone();
        let weak_pipeline_clone = weak_pipeline.clone();
        let playlist_clone2 = playlist_clone.clone();
        let playing_id_clone2 = playing_id_clone.clone();
        
        std::thread::spawn(move || {
            loop {
                if let Some(msg) = bus_clone.timed_pop(gst::ClockTime::from_mseconds(100)) {
                    if let Some(p) = weak_pipeline_clone.upgrade() {
                        match msg.view() {
                            gst::MessageView::Error(err) => {
                                eprintln!("[GStreamer Error] from {:?}: {}", 
                                        err.src().map(|s| s.path_string()), err.error());
                            }
                            gst::MessageView::Application(app_msg) => {
                                if app_msg.structure().map_or(false, |s| s.name() == "hayai-playlist-eos") {
                                    println!("[hayai] Received EOS signal, switching to next source.");
                                    let old_src_name = app_msg.structure().unwrap()
                                        .get::<String>("source-name").unwrap();
                                    let old_src = p.by_name(&old_src_name);
                                    
                                    // Get the selectors
                                    let vs = p.by_name("video_selector").unwrap();
                                    let as_ = p.by_name("audio_selector").unwrap();
                                    
                                    if let Err(e) = play_next(&p, &vs, &as_, &playlist_clone2, &playing_id_clone2, old_src) {
                                        eprintln!("[hayai] Failed to play next: {}", e);
                                    }
                                }
                            }
                            gst::MessageView::Eos(_) => {
                                println!("[hayai] Pipeline EOS received");
                                break;
                            }
                            _ => (),
                        }
                    } else {
                        // Pipeline has been dropped, exit thread
                        break;
                    }
                } else {
                    // Check if pipeline still exists
                    if weak_pipeline_clone.upgrade().is_none() {
                        break;
                    }
                }
            }
        });
        
        // Start the first item
        let vs = pipeline.by_name("video_selector").unwrap();
        let as_ = pipeline.by_name("audio_selector").unwrap();
        
        if let Err(e) = play_next(&pipeline, &vs, &as_, &self.playlist, &self.currently_playing_id, None) {
            return Err(anyhow!("Failed to prepare first item: {}", e));
        }
        
        pipeline.set_state(gst::State::Playing)?;
        self.pipeline = Some(pipeline);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(pipeline) = self.pipeline.take() { 
            pipeline.set_state(gst::State::Null)?; 
        }
        *self.currently_playing_id.lock().unwrap() = None;
        Ok(())
    }
    
    pub fn add_item(&self, uri: &str) {
        let mut playlist = self.playlist.lock().unwrap();
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        playlist.push(PlaylistItem { id, uri: uri.to_string() });
    }
    
    pub fn remove_item(&self, id: u64) { 
        self.playlist.lock().unwrap().retain(|item| item.id != id); 
    }
    
    pub fn move_item(&self, id: u64, new_index: usize) -> Result<()> {
        let mut playlist = self.playlist.lock().unwrap();
        if new_index >= playlist.len() { 
            return Err(anyhow!("Index out of bounds")); 
        }
        let old_index = playlist.iter().position(|item| item.id == id)
            .ok_or_else(|| anyhow!("ID not found"))?;
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
}

fn create_processing_bin(rtmp_url: &str, settings: &EncodingSettings) -> Result<gst::Bin> {
    let bin = gst::Bin::with_name("processing_bin");
    let vconv = gst::ElementFactory::make("videoconvert").build()?;
    let vrate = gst::ElementFactory::make("videorate").build()?;
    let venc = gst::ElementFactory::make(&settings.video_encoder).name("video_encoder").build()?;
    let aconv = gst::ElementFactory::make("audioconvert").build()?;
    let aresample = gst::ElementFactory::make("audioresample").build()?;
    let aenc = gst::ElementFactory::make(&settings.audio_encoder).build()?;
    let mux = gst::ElementFactory::make("flvmux").name("mux").property("streamable", true).build()?;
    let sink = gst::ElementFactory::make("rtmpsink").build()?;
    
    // Configure encoders
    if venc.has_property("tune") { venc.set_property_from_str("tune", "zerolatency"); }
    if venc.has_property("bitrate") { venc.set_property("bitrate", settings.bitrate_kbps); }
    if venc.has_property("speed-preset") { venc.set_property_from_str("speed-preset", &settings.speed_preset); }
    if venc.has_property("key-int-max") { venc.set_property("key-int-max", 60u32); }
    if aenc.has_property("bitrate") { aenc.set_property("bitrate", 128000_i32); }
    sink.set_property("location", rtmp_url);
    sink.set_property("sync", false);
    sink.set_property("qos", true);
    
    if settings.scale_enabled {
        let vscale = gst::ElementFactory::make("videoscale").build()?;
        let capsfilter = gst::ElementFactory::make("capsfilter").build()?;
        let caps = gst::Caps::builder("video/x-raw")
            .field("width", settings.scale_width as i32)
            .field("height", settings.scale_height as i32)
            .build();
        capsfilter.set_property("caps", caps);
        bin.add_many(&[&vconv, &vrate, &vscale, &capsfilter, &venc, &aconv, &aresample, &aenc, &mux, &sink])?;
        gst::Element::link_many(&[&vconv, &vrate, &vscale, &capsfilter, &venc, &mux])?;
    } else {
        bin.add_many(&[&vconv, &vrate, &venc, &aconv, &aresample, &aenc, &mux, &sink])?;
        gst::Element::link_many(&[&vconv, &vrate, &venc, &mux])?;
    }
    gst::Element::link_many(&[&aconv, &aresample, &aenc, &mux])?;
    mux.link(&sink)?;
    
    // Create ghost pads
    let vpad = gst::GhostPad::with_target(&vconv.static_pad("sink").unwrap())?;
    vpad.set_property("name", "video_sink");
    bin.add_pad(&vpad)?;
    let apad = gst::GhostPad::with_target(&aconv.static_pad("sink").unwrap())?;
    apad.set_property("name", "audio_sink");
    bin.add_pad(&apad)?;

    Ok(bin)
}

fn play_next(
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
    drop(playlist);

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
    
    // FIXED: Use uridecodebin instead of rtmpsink
    let source_elem = gst::ElementFactory::make("uridecodebin")
        .name(&format!("source_elem_{}", item.id))
        .build()?;
    source_elem.set_property("uri", &item.uri);  // FIXED: Use "uri" property

    pipeline.add(&source_elem)?;
    
    let v_selector_clone = v_selector.clone();
    let a_selector_clone = a_selector.clone();
    let bus = pipeline.bus().unwrap();
    let source_name = source_elem.name().to_string();
    
    source_elem.connect_pad_added(move |_src, pad| {
        println!("[DEBUG] pad-added: Fired for pad '{}'", pad.name());
        if let Some(caps) = pad.current_caps() {
            if let Some(s) = caps.structure(0) {
                let media_type = s.name();
                println!("[DEBUG] pad-added: Media type is '{}'", media_type);
                
                if media_type.starts_with("video/") {
                    let sink_pad = v_selector_clone.request_pad_simple("sink_%u").unwrap();
                    println!("[DEBUG] pad-added: Linking video pad to selector pad '{}'", sink_pad.name());
                    if let Err(e) = pad.link(&sink_pad) { 
                        eprintln!("[hayai] Failed to link video pad: {}", e); 
                    } else { 
                        v_selector_clone.set_property("active-pad", &sink_pad); 
                    }
                } else if media_type.starts_with("audio/") {
                    let sink_pad = a_selector_clone.request_pad_simple("sink_%u").unwrap();
                    println!("[DEBUG] pad-added: Linking audio pad to selector pad '{}'", sink_pad.name());
                    if let Err(e) = pad.link(&sink_pad) { 
                        eprintln!("[hayai] Failed to link audio pad: {}", e); 
                    } else { 
                        a_selector_clone.set_property("active-pad", &sink_pad); 
                    }
                }
                
                // CRITICAL: Add EOS detection probe
                let bus_clone = bus.clone();
                let source_name_clone = source_name.clone();
                pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, probe_info| {
                    if let Some(gst::PadProbeData::Event(event)) = &probe_info.data {
                        if event.type_() == gst::EventType::Eos {
                            println!("[hayai] Pad probe detected EOS for {}!", source_name_clone);
                            let s = gst::Structure::builder("hayai-playlist-eos")
                                .field("source-name", &source_name_clone)
                                .build();
                            let msg = gst::message::Application::new(s);
                            let _ = bus_clone.post(msg);
                        }
                    }
                    gst::PadProbeReturn::Ok
                });
            }
        }
    });

    // Clean up old source
    if let Some(old_elem) = old_source {
        println!("[DEBUG] switch_source: Scheduling cleanup for old element: {}", old_elem.name());
        let pipeline_clone = pipeline.clone();
        let v_selector_clone = v_selector.clone();
        let a_selector_clone = a_selector.clone();
        
        pipeline.call_async(move |_| {
            println!("[DEBUG] call_async: Now cleaning up old element '{}'", old_elem.name());
            
            let _ = old_elem.set_state(gst::State::Null);
            
            // Release selector pads
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
            
            let _ = pipeline_clone.remove(&old_elem);
        });
    }
    
    source_elem.sync_state_with_parent()?;
    println!("[DEBUG] switch_source: New source '{}' is now synchronized.", item.uri);
    Ok(())
}

impl Drop for Streamer {
    fn drop(&mut self) {
        if self.pipeline.is_some() { 
            let _ = self.stop(); 
        }
    }
}