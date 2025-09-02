use anyhow::Result;
use hayai_playout_core::{EncodingSettings, Streamer};
use std::sync::{Arc, Mutex};

use gstreamer as gst;
use gstreamer::prelude::*;
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box, Button, CheckButton, ComboBoxText, Entry,
    FileChooserAction, FileChooserDialog, Grid, Label, ListBox, ListBoxRow, MessageDialog, MessageType,
    Orientation, PolicyType, ResponseType, ScrolledWindow, SpinButton,
};

fn main() -> Result<()> {
    gst::init()?;
    lower_nvdec_rank();
    let streamer = Arc::new(Mutex::new(Streamer::new()?));
    let app = Application::new(Some("com.example.hayaipLayout"), Default::default());
    app.connect_activate(move |app| {
        build_ui(app, streamer.clone());
    });
    app.run();
    Ok(())
}

fn lower_nvdec_rank() {
    let registry = gst::Registry::get();
    for factory in registry.features(gst::ElementFactory::static_type()) {
        if let Some(factory) = factory.downcast_ref::<gst::ElementFactory>() {
            if factory.name().starts_with("nv") {
                factory.set_rank(gst::Rank::NONE);
            }
        }
    }
}

fn show_error_dialog(parent: &ApplicationWindow, text: &str) {
    let dialog = MessageDialog::new(
        Some(parent),
        gtk::DialogFlags::MODAL,
        MessageType::Error,
        gtk::ButtonsType::Ok,
        "Failed to Start Stream",
    );
    dialog.set_secondary_text(Some(text));
    dialog.connect_response(|d, _| d.close());
    dialog.show();
}

fn get_available_encoders(klass: &str) -> Vec<String> {
    let mut encoders = Vec::new();
    let registry = gst::Registry::get();
    for factory in registry.features(gst::ElementFactory::static_type()) {
        if let Some(factory) = factory.downcast_ref::<gst::ElementFactory>() {
            if factory.klass().contains(klass) {
                encoders.push(factory.name().to_string());
            }
        }
    }
    encoders.sort();
    encoders
}

fn build_ui(app: &Application, streamer: Arc<Mutex<Streamer>>) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Hayai Playout")
        .default_width(400)
        .default_height(600)
        .build();

    let settings_grid = Grid::builder()
        .margin_top(10).margin_bottom(10).margin_start(10).margin_end(10)
        .column_spacing(10).row_spacing(10)
        .build();
    
    let settings_label = Label::new(None);
    settings_label.set_markup("<b>Encoding Settings</b>");
    settings_grid.attach(&settings_label, 0, 0, 2, 1);

    settings_grid.attach(&Label::new(Some("Video Encoder:")), 0, 1, 1, 1);
    let video_encoder_combo = ComboBoxText::new();
    let available_video_encoders = get_available_encoders("Codec/Encoder/Video");
    for enc in &available_video_encoders {
        video_encoder_combo.append_text(enc);
    }
    if let Some(idx) = available_video_encoders.iter().position(|r| r == "x264enc") {
        video_encoder_combo.set_active(Some(idx as u32));
    }

    settings_grid.attach(&video_encoder_combo, 1, 1, 1, 1);
    
    settings_grid.attach(&Label::new(Some("Audio Encoder:")), 0, 2, 1, 1);
    let audio_encoder_combo = ComboBoxText::new();
    let available_audio_encoders = get_available_encoders("Codec/Encoder/Audio");
    for enc in &available_audio_encoders {
        audio_encoder_combo.append_text(enc);
    }
    if let Some(idx) = available_audio_encoders.iter().position(|r| r == "faac") {
        audio_encoder_combo.set_active(Some(idx as u32));
    } else if !available_audio_encoders.is_empty() {
        audio_encoder_combo.set_active(Some(0));
    }
    settings_grid.attach(&audio_encoder_combo, 1, 2, 1, 1);

    settings_grid.attach(&Label::new(Some("Bitrate (kbps):")), 0, 3, 1, 1);
    let bitrate_spin = SpinButton::with_range(500.0, 20000.0, 500.0);
    bitrate_spin.set_value(4000.0);
    settings_grid.attach(&bitrate_spin, 1, 3, 1, 1);

    settings_grid.attach(&Label::new(Some("Preset:")), 0, 4, 1, 1);
    let preset_combo = ComboBoxText::new();
    let presets = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium"];
    for p in presets {
        preset_combo.append_text(p);
    }
    preset_combo.set_active(Some(0));
    settings_grid.attach(&preset_combo, 1, 4, 1, 1);

    let scale_check = CheckButton::with_label("Scale Output Resolution");
    settings_grid.attach(&scale_check, 0, 5, 2, 1);

    settings_grid.attach(&Label::new(Some("Width:")), 0, 6, 1, 1);
    let width_spin = SpinButton::with_range(1.0, 7680.0, 1.0);
    width_spin.set_value(1920.0);
    width_spin.set_sensitive(false);
    settings_grid.attach(&width_spin, 1, 6, 1, 1);
    
    settings_grid.attach(&Label::new(Some("Height:")), 0, 7, 1, 1);
    let height_spin = SpinButton::with_range(1.0, 4320.0, 1.0);
    height_spin.set_value(1080.0);
    height_spin.set_sensitive(false);
    settings_grid.attach(&height_spin, 1, 7, 1, 1);

    scale_check.connect_toggled({
        let width_spin = width_spin.clone();
        let height_spin = height_spin.clone();
        move |check| {
            let is_active = check.is_active();
            width_spin.set_sensitive(is_active);
            height_spin.set_sensitive(is_active);
        }
    });

    let main_vbox = Box::new(Orientation::Vertical, 5);
    let rtmp_entry = Entry::builder().placeholder_text("rtmp://...").margin_start(10).margin_end(10).build();
    let playlist_box = ListBox::new();
    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never).min_content_height(300)
        .vexpand(true).child(&playlist_box).build();
    let button_hbox = Box::new(Orientation::Horizontal, 5);
    button_hbox.set_halign(Align::Center);
    button_hbox.set_margin_bottom(10);
    
    let add_button = Button::with_label("Add File");
    let move_up_button = Button::with_label("Move Up");
    let move_down_button = Button::with_label("Move Down");
    let start_button = Button::with_label("Start");
    let stop_button = Button::with_label("Stop");
    stop_button.set_sensitive(false);
    move_up_button.set_sensitive(false);
    move_down_button.set_sensitive(false);

    button_hbox.append(&add_button);
    button_hbox.append(&move_up_button);
    button_hbox.append(&move_down_button);
    button_hbox.append(&start_button);
    button_hbox.append(&stop_button);
    
    main_vbox.append(&settings_grid);
    main_vbox.append(&rtmp_entry);
    main_vbox.append(&scrolled_window);
    main_vbox.append(&button_hbox);
    window.set_child(Some(&main_vbox));

    let selected_index = Arc::new(Mutex::new(None::<u32>));
    let update_playlist_view = {
        let playlist_box = playlist_box.clone();
        let streamer = streamer.clone();
        let selected_index = selected_index.clone();
        move || {
            let mut current_sel = selected_index.lock().unwrap();
            while let Some(child) = playlist_box.first_child() { playlist_box.remove(&child); }
            let playlist = streamer.lock().unwrap().get_playlist_clone();
            for item in playlist {
                let label = Label::new(Some(&item.uri));
                let row = ListBoxRow::builder().child(&label).build();
                playlist_box.append(&row);
            }
            if let Some(idx) = *current_sel {
                if let Some(row) = playlist_box.row_at_index(idx as i32) {
                    playlist_box.select_row(Some(&row));
                }
            }
        }
    };

    playlist_box.connect_row_selected({
        let move_up = move_up_button.clone();
        let move_down = move_down_button.clone();
        let selected_index = selected_index.clone();
        move |box_, row| {
            let mut idx_opt = selected_index.lock().unwrap();
            if let Some(row) = row {
                let idx = row.index() as u32;
                *idx_opt = Some(idx);
                move_up.set_sensitive(idx > 0);
                move_down.set_sensitive(idx < (box_.observe_children().n_items() - 1));
            } else {
                *idx_opt = None;
                move_up.set_sensitive(false);
                move_down.set_sensitive(false);
            }
        }
    });

    let window_clone = window.clone();
    add_button.connect_clicked({
        let streamer = streamer.clone();
        let update_playlist_view = update_playlist_view.clone();
        move |_| {
            let file_chooser = FileChooserDialog::new(
                Some("Select a Video File"),
                Some(&window_clone),
                FileChooserAction::Open,
                &[("Open", ResponseType::Accept), ("Cancel", ResponseType::Cancel)],
            );
            file_chooser.connect_response({
                let streamer = streamer.clone();
                let update_playlist_view = update_playlist_view.clone();
                move |dialog, response| {
                    if response == ResponseType::Accept {
                        if let Some(file) = dialog.file() {
                            let uri = file.uri();
                            streamer.lock().unwrap().add_item(uri.as_str());
                            update_playlist_view();
                        }
                    }
                    dialog.close();
                }
            });
            file_chooser.show();
        }
    });

    start_button.connect_clicked({
        let streamer = streamer.clone();
        let window = window.clone();
        let video_encoder_combo = video_encoder_combo.clone();
        let audio_encoder_combo = audio_encoder_combo.clone();
        let bitrate_spin = bitrate_spin.clone();
        let preset_combo = preset_combo.clone();
        let scale_check = scale_check.clone();
        let width_spin = width_spin.clone();
        let height_spin = height_spin.clone();
        let rtmp_entry = rtmp_entry.clone();
        let stop_button = stop_button.clone();

        move |start_button| {
            let rtmp_url = rtmp_entry.text();
            if rtmp_url.is_empty() { 
                show_error_dialog(&window, "RTMP URL cannot be empty.");
                return; 
            }

            let settings = EncodingSettings {
                video_encoder: video_encoder_combo.active_text().unwrap_or_default().to_string(),
                audio_encoder: audio_encoder_combo.active_text().unwrap_or_default().to_string(),
                bitrate_kbps: bitrate_spin.value() as u32,
                speed_preset: preset_combo.active_text().unwrap_or_default().to_string(),
                scale_enabled: scale_check.is_active(),
                scale_width: width_spin.value() as u32,
                scale_height: height_spin.value() as u32,
            };
            
            match streamer.lock().unwrap().start(&rtmp_url, &settings) {
                Ok(_) => {
                    println!("Stream started successfully!");
                    start_button.set_sensitive(false);
                    stop_button.set_sensitive(true);
                    video_encoder_combo.set_sensitive(false);
                    audio_encoder_combo.set_sensitive(false);
                    bitrate_spin.set_sensitive(false);
                    preset_combo.set_sensitive(false);
                    scale_check.set_sensitive(false);
                    width_spin.set_sensitive(false);
                    height_spin.set_sensitive(false);
                    rtmp_entry.set_sensitive(false);
                },
                Err(e) => show_error_dialog(&window, &e.to_string()),
            }
        }
    });

    stop_button.connect_clicked({
        let streamer = streamer.clone();
        let start_button = start_button.clone();
        let video_encoder_combo = video_encoder_combo.clone();
        let audio_encoder_combo = audio_encoder_combo.clone();
        let bitrate_spin = bitrate_spin.clone();
        let preset_combo = preset_combo.clone();
        let scale_check = scale_check.clone();
        let width_spin = width_spin.clone();
        let height_spin = height_spin.clone();
        let rtmp_entry = rtmp_entry.clone();

        move |stop_button| {
             match streamer.lock().unwrap().stop() {
                Ok(_) => {
                    println!("Stream stopped.");
                    stop_button.set_sensitive(false);
                    start_button.set_sensitive(true);
                    video_encoder_combo.set_sensitive(true);
                    audio_encoder_combo.set_sensitive(true);
                    bitrate_spin.set_sensitive(true);
                    preset_combo.set_sensitive(true);
                    scale_check.set_sensitive(true);
                    let is_scale_active = scale_check.is_active();
                    width_spin.set_sensitive(is_scale_active);
                    height_spin.set_sensitive(is_scale_active);
                    rtmp_entry.set_sensitive(true);
                },
                Err(e) => eprintln!("Failed to stop stream: {}", e),
            }
        }
    });

    move_up_button.connect_clicked({
        let streamer = streamer.clone();
        let update_playlist_view = update_playlist_view.clone();
        let selected_index = selected_index.clone();
        move |_| {
            let mut idx_opt = selected_index.lock().unwrap();
            if let Some(idx) = *idx_opt {
                if idx > 0 {
                    let playlist = streamer.lock().unwrap().get_playlist_clone();
                    let item_id = playlist[idx as usize].id;
                    let new_idx = idx - 1;
                    if streamer.lock().unwrap().move_item(item_id, new_idx as usize).is_ok() {
                        *idx_opt = Some(new_idx);
                        update_playlist_view();
                    }
                }
            }
        }
    });

    move_down_button.connect_clicked({
        let streamer = streamer.clone();
        let update_playlist_view = update_playlist_view.clone();
        let selected_index = selected_index.clone();
        move |_| {
            let mut idx_opt = selected_index.lock().unwrap();
            if let Some(idx) = *idx_opt {
                let playlist = streamer.lock().unwrap().get_playlist_clone();
                if idx < (playlist.len() - 1) as u32 {
                    let item_id = playlist[idx as usize].id;
                    let new_idx = idx + 1;
                    if streamer.lock().unwrap().move_item(item_id, new_idx as usize).is_ok() {
                        *idx_opt = Some(new_idx);
                        update_playlist_view();
                    }
                }
            }
        }
    });

    window.present();
}