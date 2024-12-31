use std::process::Command;
use std::time::Duration;

use chrono::{DateTime, Utc, Timelike};
use clap::Parser;
use serde::Deserialize;
use tokio::time::sleep;
use nokhwa::{
    Camera,
    utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType},
    pixel_format::RgbFormat
};
use dotenv::dotenv;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// OpenWeather API key for weather data (required only if webcam is not available)
    #[arg(long)]
    api_key: Option<String>,

    /// Minimum brightness level (0.0 to 1.0)
    #[arg(long, default_value_t = 0.6)]
    min_brightness: f64,

    /// Color temperature during day (Kelvin)
    #[arg(long, default_value_t = 6500.0)]
    day_temp: f64,

    /// Color temperature during night (Kelvin)
    #[arg(long, default_value_t = 3500.0)]
    night_temp: f64,

    /// Hours before sunset to start transitioning
    #[arg(long, default_value_t = 2.0)]
    transition_hours: f64,

    /// Comma-separated list of monitor names (e.g., "DP-0,HDMI-0")
    #[arg(long, value_delimiter = ',')]
    monitors: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WeatherApiResponse {
    sys: SysInfo,
    clouds: CloudInfo,
}

#[derive(Debug, Deserialize)]
struct SysInfo {
    sunrise: i64,
    sunset: i64,
}

#[derive(Debug, Deserialize)]
struct CloudInfo {
    all: f64,  // cloud coverage in percentage
}

#[derive(Debug, Deserialize)]
struct LocationApiResponse {
    lat: f64,
    lon: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Try webcam first
    match detect_brightness_from_webcam(args.min_brightness) {
        Ok(brightness) => {
            if let Err(e) = set_monitor_brightness(brightness, &args) {
                eprintln!("Failed to set brightness: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Webcam not available ({}), falling back to weather API", e);
            
            // Check if API key is provided when falling back to weather API
            let api_key = args.api_key.clone().ok_or("OpenWeather API key is required when webcam is not available")?;
            
            // Fall back to weather API
            let location = fetch_location().await?;
            let lat = location.lat.to_string();
            let lon = location.lon.to_string();

            match fetch_weather(&lat, &lon, &api_key).await {
                Ok(weather_data) => {
                    let brightness = compute_brightness(&weather_data, args.min_brightness);
                    if let Err(e) = set_monitor_brightness(brightness, &args) {
                        eprintln!("Failed to set brightness: {}", e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to fetch weather data: {}", e);
                },
            }
        }
    }

    Ok(())
}

async fn fetch_weather(
    lat: &str,
    lon: &str,
    api_key: &str,
) -> Result<WeatherApiResponse, Box<dyn std::error::Error>> {
    // Example OpenWeatherMap endpoint
    let url = format!(
        "https://api.openweathermap.org/data/2.5/weather?lat={}&lon={}&appid={}",
        lat, lon, api_key
    );

    let resp = reqwest::get(&url).await?.json::<WeatherApiResponse>().await?;
    Ok(resp)
}

async fn fetch_location() -> Result<LocationApiResponse, Box<dyn std::error::Error>> {
    let url = "http://ip-api.com/json";
    let resp = reqwest::get(url).await?.json::<LocationApiResponse>().await?;
    Ok(resp)
}

/// Computes a simplistic “outside brightness” factor [0.0..1.0]
/// based on sunrise/sunset times and cloud coverage.
fn compute_brightness(weather: &WeatherApiResponse, min_brightness: f64) -> f64 {
    let now_utc: DateTime<Utc> = Utc::now();
    let now_ts = now_utc.timestamp();

    let sunrise = weather.sys.sunrise;
    let sunset = weather.sys.sunset;
    let cloud_cover = weather.clouds.all;

    if now_ts < sunrise || now_ts > sunset {
        return min_brightness;
    }

    let day_length = (sunset - sunrise) as f64;
    let time_since_sunrise = (now_ts - sunrise) as f64;
    let mut fraction_of_day = time_since_sunrise / day_length;

    fraction_of_day = fraction_of_day.clamp(0.0, 1.0);

    let midday_bump = if fraction_of_day <= 0.5 {
        fraction_of_day * 2.0
    } else {
        (1.0 - fraction_of_day) * 2.0
    };

    let cloud_factor = 1.0 - (cloud_cover / 100.0);
    let outside_brightness = midday_bump * cloud_factor;

    min_brightness + outside_brightness * (1.0 - min_brightness)
}

/// Sets brightness and color temperature for monitors using xrandr
fn set_monitor_brightness(brightness: f64, args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let monitors = match &args.monitors {
        Some(m) => m.clone(),
        None => detect_monitors()?
    };

    let now_utc: DateTime<Utc> = Utc::now();
    let now_local = now_utc.with_timezone(&chrono::Local);
    let hour = now_local.hour() as f64 + (now_local.minute() as f64 / 60.0);

    let color_temp = if hour >= 18.0 || hour <= 6.0 {
        args.night_temp
    } else if hour >= (18.0 - args.transition_hours) && hour < 18.0 {
        let progress = (18.0 - hour) / args.transition_hours;
        args.day_temp * progress + args.night_temp * (1.0 - progress)
    } else {
        args.day_temp
    };

    let (r_gamma, g_gamma, b_gamma) = temp_to_gamma(color_temp);

    for monitor in &monitors {
        match Command::new("xrandr")
            .args(&[
                "--output", monitor,
                "--brightness", &format!("{:.3}", brightness),
                "--gamma", &format!("{:.3}:{:.3}:{:.3}", r_gamma, g_gamma, b_gamma)
            ])
            .status()
        {
            Ok(status) if !status.success() => {
                eprintln!("Failed to set brightness/gamma for {}: {:?}", monitor, status);
            }
            Err(e) => {
                eprintln!("Error setting brightness/gamma for {}: {}", monitor, e);
            }
            _ => {}
        }
    }

    Ok(())
}

/// Convert color temperature (in Kelvin) to RGB gamma values
fn temp_to_gamma(temp: f64) -> (f64, f64, f64) {
    let temp = temp / 100.0;

    let red = if temp <= 66.0 {
        1.0
    } else {
        let t = temp - 60.0;
        (1.29293618606274514 * t.powf(-0.1332047592)).clamp(0.0, 1.0)
    };

    let green = if temp <= 66.0 {
        let t = temp;
        (0.39008157876901960784 * (t.ln()) - 0.631841443788046).clamp(0.0, 1.0)
    } else {
        let t = temp - 60.0;
        (1.12989086089529411765 * t.powf(-0.0755148492)).clamp(0.0, 1.0)
    };

    let blue = if temp >= 66.0 {
        1.0
    } else if temp <= 19.0 {
        0.0
    } else {
        let t = temp - 10.0;
        (0.54320678911019607843 * (t.ln()) - 1.19625408914).clamp(0.0, 1.0)
    };

    (red, green, blue)
}

/// Captures an image from webcam and computes average brightness
fn detect_brightness_from_webcam(min_brightness: f64) -> Result<f64, Box<dyn std::error::Error>> {
    let mut camera = Camera::new(
        CameraIndex::Index(0),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(
            CameraFormat::new_from(640, 480, FrameFormat::MJPEG, 30)
        ))
    )?;

    camera.open_stream()?;

    for _ in 0..5 {
        let _ = camera.frame()?;
        sleep(Duration::from_millis(100));
    }

    let frame = camera.frame()?;
    let img = frame.decode_image::<RgbFormat>()?;

    camera.stop_stream()?;

    let mut total_brightness = 0.0;
    let pixels = img.pixels();
    let pixel_count = pixels.len() as f64;

    for pixel in pixels {
        let r = pixel[0] as f64;
        let g = pixel[1] as f64;
        let b = pixel[2] as f64;
        total_brightness += (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
    }

    let avg_brightness = total_brightness / pixel_count;
    let clamped_brightness = avg_brightness.clamp(0.0, 1.0);
    let screen_brightness = min_brightness + (clamped_brightness * (1.0 - min_brightness));

    Ok(screen_brightness)
}

/// Detect available monitors using xrandr
fn detect_monitors() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("xrandr")
        .arg("--listmonitors")
        .output()?;

    if !output.status.success() {
        return Err("Failed to execute xrandr --listmonitors".into());
    }

    let output_str = String::from_utf8(output.stdout)?;
    let monitors: Vec<String> = output_str
        .lines()
        .skip(1)  // Skip the first line (contains count)
        .filter_map(|line| {
            line.split_whitespace()
                .last()
                .map(String::from)
        })
        .collect();

    if monitors.is_empty() {
        return Err("No monitors detected".into());
    }

    Ok(monitors)
}
