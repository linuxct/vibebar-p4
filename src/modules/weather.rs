use gtk4::prelude::*;
use gtk4::{Label, Popover, PositionType, EventControllerMotion};
use std::time::Duration;
use std::process::Command;
use serde_json::Value;
use chrono::Timelike;

const WEATHER_CODES: &[(&str, &str)] = &[
    ("113", "☀️"), ("116", "⛅️"), ("119", "☁️"), ("122", "☁️"),
    ("143", "🌫"), ("176", "🌦"), ("179", "🌧"), ("182", "🌧"),
    ("185", "🌧"), ("200", "⛈"), ("227", "🌨"), ("230", "❄️"),
    ("248", "🌫"), ("260", "🌫"), ("263", "🌦"), ("266", "🌦"),
    ("281", "🌧"), ("284", "🌧"), ("293", "🌦"), ("296", "🌦"),
    ("299", "🌧"), ("302", "🌧"), ("305", "🌧"), ("308", "🌧"),
    ("311", "🌧"), ("314", "🌧"), ("317", "🌧"), ("320", "🌨"),
    ("323", "🌨"), ("326", "🌨"), ("329", "❄️"), ("332", "❄️"),
    ("335", "❄️"), ("338", "❄️"), ("350", "🌧"), ("353", "🌦"),
    ("356", "🌧"), ("359", "🌧"), ("362", "🌧"), ("365", "🌧"),
    ("368", "🌨"), ("371", "❄️"), ("374", "🌧"), ("377", "🌧"),
    ("386", "⛈"), ("389", "🌩"), ("392", "⛈"), ("395", "❄️"),
];

fn get_emoji(code: &str) -> &str {
    WEATHER_CODES.iter()
        .find(|(c, _)| *c == code)
        .map(|(_, e)| *e)
        .unwrap_or("❓")
}

pub fn init(container: &gtk4::Box) {
    let label = Label::builder()
        .label("✨ ...")
        .build();
    label.set_widget_name("weather-module");
    container.append(&label);

    let popover = Popover::builder()
        .position(PositionType::Top)
        .autohide(false)
        .has_arrow(true)
        .build();
    popover.set_parent(&label);

    // Explicitly point to the top of the label
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(0, 0, 100, 1)));

    let popover_label = Label::builder()
        .use_markup(true)
        .build();
    popover_label.set_widget_name("popover-label");
    popover.set_child(Some(&popover_label));

    let motion_controller = EventControllerMotion::new();
    let p_enter = popover.clone();
    motion_controller.connect_enter(move |_, _, _| {
        p_enter.popup();
    });
    let p_leave = popover.clone();
    motion_controller.connect_leave(move |_| {
        p_leave.popdown();
    });
    label.add_controller(motion_controller);

    let (refresh_tx, refresh_rx) = std::sync::mpsc::channel::<()>();
    let click_gesture = gtk4::GestureClick::new();
    click_gesture.set_button(0); // All buttons
    click_gesture.connect_pressed(move |_, _, _, _| {
        let _ = refresh_tx.send(());
    });
    label.add_controller(click_gesture);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<WeatherUpdate>();
    
    let l = label.clone();
    let pl = popover_label.clone();
    gtk4::glib::MainContext::default().spawn_local(async move {
        while let Some(update) = rx.recv().await {
            l.set_label(&update.text);
            pl.set_markup(&update.tooltip);
        }
    });

    std::thread::spawn(move || {
        let mut retry_interval = Duration::from_secs(0); // Start immediately
        loop {
            // Wait for retry interval OR refresh signal
            let _ = refresh_rx.recv_timeout(retry_interval);

            let update = fetch_weather();
            match update {
                Ok(u) => {
                    let _ = tx.send(u);
                    retry_interval = Duration::from_secs(900); // Success, wait 15 mins
                }
                Err(_) => {
                    retry_interval = Duration::from_secs(15); // Failure/Timeout, retry in 15s
                }
            }
        }
    });
}

struct WeatherUpdate {
    text: String,
    tooltip: String,
}

fn format_chances(hour: &Value) -> String {
    let chances = [
        ("chanceoffog", "Fog"),
        ("chanceoffrost", "Frost"),
        ("chanceofovercast", "Overcast"),
        ("chanceofrain", "Rain"),
        ("chanceofsnow", "Snow"),
        ("chanceofsunshine", "Sunshine"),
        ("chanceofthunder", "Thunder"),
        ("chanceofwindy", "Wind"),
    ];

    let mut conditions = Vec::new();
    for (key, label) in chances {
        if let Some(val_str) = hour[key].as_str() {
            if let Ok(val) = val_str.parse::<i32>() {
                if val > 0 {
                    conditions.push(format!("{} {}%", label, val));
                }
            }
        }
    }
    conditions.join(", ")
}

fn fetch_weather() -> Result<WeatherUpdate, Box<dyn std::error::Error>> {
    let output = Command::new("ip").arg("address").output()?;
    let ip_out = String::from_utf8_lossy(&output.stdout);
    let location = if ip_out.contains("192.168.10") { "Cartagena" } else { "Murcia" };

    let url = format!("https://wttr.in/{}?format=j1", location);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let res = client.get(url).send()?.json::<Value>()?;
    let data = res.get("data").unwrap_or(&res);

    let current = &data["current_condition"][0];
    let code = current["weatherCode"].as_str().unwrap_or("");
    let temp = current["temp_C"].as_str().unwrap_or("?");
    let desc = current["weatherDesc"][0]["value"].as_str().unwrap_or("");
    let feels = current["FeelsLikeC"].as_str().unwrap_or("?");
    let wind = current["windspeedKmph"].as_str().unwrap_or("?");
    let humidity = current["humidity"].as_str().unwrap_or("?");

    let emoji = get_emoji(code);
    let text = format!("{}  {}°", emoji, temp);

    let mut tooltip = format!("<tt><b>{} {}°</b>\n", desc, temp);
    tooltip.push_str(&format!("Feels like: {}°\n", feels));
    tooltip.push_str(&format!("Wind: {}Km/h\n", wind));
    tooltip.push_str(&format!("Humidity: {}%\n", humidity));

    let now_hour = chrono::Local::now().hour() as i32;

    if let Some(days) = data["weather"].as_array() {
        for (i, day) in days.iter().enumerate().take(3) {
            let date = day["date"].as_str().unwrap_or("");
            let max = day["maxtempC"].as_str().unwrap_or("?");
            let min = day["mintempC"].as_str().unwrap_or("?");
            let sunrise = day["astronomy"][0]["sunrise"].as_str().unwrap_or("?");
            let sunset = day["astronomy"][0]["sunset"].as_str().unwrap_or("?");

            tooltip.push_str("\n<b>");
            if i == 0 { tooltip.push_str("Today, "); }
            else if i == 1 { tooltip.push_str("Tomorrow, "); }
            tooltip.push_str(&format!("{}</b>\n", date));
            tooltip.push_str(&format!("⬆️ {}° ⬇️ {}° ", max, min));
            tooltip.push_str(&format!("🌅 {} 🌇 {}\n", sunrise, sunset));

            if let Some(hourly) = day["hourly"].as_array() {
                for hour in hourly {
                    let time_raw = hour["time"].as_str().unwrap_or("0");
                    let time_h = time_raw.parse::<i32>().unwrap_or(0) / 100;
                    
                    if i == 0 && time_h < now_hour - 2 {
                        continue;
                    }

                    let h_code = hour["weatherCode"].as_str().unwrap_or("");
                    let h_emoji = get_emoji(h_code);
                    let h_feels = hour["FeelsLikeC"].as_str().unwrap_or("?");
                    let h_desc = hour["weatherDesc"][0]["value"].as_str().unwrap_or("");
                    let h_chances = format_chances(hour);
                    
                    let time_str = format!("{:02}", time_h);
                    tooltip.push_str(&format!("{} {} {:>3}° {}", time_str, h_emoji, h_feels, h_desc));
                    if !h_chances.is_empty() {
                        tooltip.push_str(&format!(", {}", h_chances));
                    }
                    tooltip.push('\n');
                }
            }
        }
    }

    tooltip.push_str("</tt>");
    Ok(WeatherUpdate { text, tooltip })
}
