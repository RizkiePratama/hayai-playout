use anyhow::Result;
use gstreamer as gst;
use gst::prelude::*;

use super::models::EncodingSettings;

pub(crate) fn create_processing_bin(rtmp_url: &str, settings: &EncodingSettings) -> Result<gst::Bin> {
    let bin = gst::Bin::with_name("processing_bin");

    let vqueue = gst::ElementFactory::make("queue").build()?;
    let vconv = gst::ElementFactory::make("videoconvert").build()?;
    let vrate = gst::ElementFactory::make("videorate").build()?;
    let venc = gst::ElementFactory::make(&settings.encoder)
        .name("video_encoder")
        .build()?;
    
    let aqueue = gst::ElementFactory::make("queue").build()?;
    let aconv = gst::ElementFactory::make("audioconvert").build()?;
    let aresample = gst::ElementFactory::make("audioresample").build()?;
    let aenc = gst::ElementFactory::make("voaacenc").build()?;
    let mux = gst::ElementFactory::make("flvmux")
        .name("mux")
        .property("streamable", true)
        .build()?;
    let sink = gst::ElementFactory::make("rtmpsink").build()?;
    
    if venc.has_property("tune") { venc.set_property_from_str("tune", "zerolatency"); }
    if venc.has_property("bitrate") { venc.set_property("bitrate", settings.bitrate_kbps); }
    if venc.has_property("speed-preset") { venc.set_property_from_str("speed-preset", &settings.speed_preset); }
    if venc.has_property("key-int-max") { venc.set_property("key-int-max", 60u32); }
    
    aenc.set_property("bitrate", 128000_i32);
    sink.set_property("location", rtmp_url);
    sink.set_property("sync", true);
    sink.set_property("qos", true);
    let max_lateness = gst::ClockTime::from_mseconds(500);
    sink.set_property("max-lateness", max_lateness.nseconds() as i64);

    if settings.scale_enabled {
        let vscale = gst::ElementFactory::make("videoscale").build()?;
        let capsfilter = gst::ElementFactory::make("capsfilter").build()?;
        let caps = gst::Caps::builder("video/x-raw")
            .field("width", settings.scale_width as i32)
            .field("height", settings.scale_height as i32)
            .build();
        capsfilter.set_property("caps", caps);
        bin.add_many(&[&vqueue, &vconv, &vrate, &vscale, &capsfilter, &venc, &aqueue, &aconv, &aresample, &aenc, &mux, &sink])?;
        gst::Element::link_many(&[&vqueue, &vconv, &vrate, &vscale, &capsfilter, &venc, &mux])?;
    } else {
        bin.add_many(&[&vqueue, &vconv, &vrate, &venc, &aqueue, &aconv, &aresample, &aenc, &mux, &sink])?;
        gst::Element::link_many(&[&vqueue, &vconv, &vrate, &venc, &mux])?;
    }
    gst::Element::link_many(&[&aqueue, &aconv, &aresample, &aenc, &mux])?;
    mux.link(&sink)?;

    let vpad = gst::GhostPad::with_target(&vqueue.static_pad("sink").unwrap())?;
    vpad.set_property("name", "video_sink");
    bin.add_pad(&vpad)?;
    let apad = gst::GhostPad::with_target(&aqueue.static_pad("sink").unwrap())?;
    apad.set_property("name", "audio_sink");
    bin.add_pad(&apad)?;
    Ok(bin)
}