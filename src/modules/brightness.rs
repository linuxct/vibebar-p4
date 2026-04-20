use gtk4::prelude::*;
use gtk4::{Label, EventControllerScroll, EventControllerScrollFlags, GestureClick};
use std::time::Duration;
use std::fs;
use std::process::Command;
use serde_json;

pub fn init(container: &gtk4::Box) {
    let label = Label::builder()
        .label("  ...%")
        .build();
    label.set_widget_name("brightness-module");
    container.append(&label);

    let scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    label.add_controller(scroll.clone());

    scroll.connect_scroll(move |_, _, dy| {
        if dy < 0.0 {
            let _ = Command::new("brightnessctl")
                .arg("set")
                .arg("5%+")
                .spawn();
        } else if dy > 0.0 {
            let _ = Command::new("brightnessctl")
                .arg("set")
                .arg("5%-")
                .spawn();
        }
        glib::Propagation::Stop
    });

    let click = GestureClick::new();
    label.add_controller(click.clone());
    
    click.connect_pressed(move |_, n_press, _, _| {
        if n_press == 1 {
            let _ = Command::new("swaymsg")
                .arg("output")
                .arg("eDP-1")
                .arg("toggle")
                .spawn();
        }
    });

    glib::timeout_add_local(Duration::from_millis(500), move || {
        let output = Command::new("swaymsg")
            .arg("-t")
            .arg("get_outputs")
            .arg("-r")
            .output();

        let is_off = if let Ok(output) = output {
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
            if let Some(outputs) = json.as_array() {
                outputs.iter()
                    .find(|o| o["name"] == "eDP-1")
                    .map(|o| o["active"] == false || o["power"] == false)
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        if is_off {
            label.set_label("  off");
        } else {
            let brightness = fs::read_to_string("/sys/class/backlight/amdgpu_bl1/brightness")
                .unwrap_or_else(|_| "0".to_string())
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0);
            
            let max_brightness = fs::read_to_string("/sys/class/backlight/amdgpu_bl1/max_brightness")
                .unwrap_or_else(|_| "1".to_string())
                .trim()
                .parse::<f64>()
                .unwrap_or(1.0);

            let perc = (brightness / max_brightness * 100.0).round() as i32;
            label.set_label(&format!("  {}%", perc));
        }
        
        glib::ControlFlow::Continue
    });
}
