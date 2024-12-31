# Healthy Monitor

A Rust application that automatically adjusts your monitor's brightness and color temperature based on:
1. Webcam light sensor (primary method)
2. Weather data and time of day (fallback method)

## Features
- Automatic brightness adjustment based on ambient light
- Color temperature adjustment based on time of day
- Configurable monitor settings
- Fallback to weather-based brightness when webcam is unavailable
- Customizable minimum brightness and color temperature settings

## Prerequisites
- Linux system with X11
- `xrandr` command-line tool
- Webcam (optional)
- Rust and Cargo

## Installation

Install directly from crates.io:
```bash
cargo install healthy-monitor
```

## Usage

Run the application with your OpenWeather API key:
```bash
healthy-monitor --api-key YOUR_API_KEY
```

### Command Line Options

```bash
healthy-monitor [OPTIONS] --api-key <API_KEY>

Options:
    --api-key <API_KEY>            OpenWeather API key for weather data [env: OPEN_WEATHER_API_KEY]
    --min-brightness <FLOAT>       Minimum brightness level (0.0 to 1.0) [default: 0.6]
    --day-temp <FLOAT>            Color temperature during day in Kelvin [default: 6500]
    --night-temp <FLOAT>          Color temperature during night in Kelvin [default: 3500]
    --transition-hours <FLOAT>     Hours before sunset to start transitioning [default: 2.0]
    --monitors <MONITORS>          Comma-separated list of monitor names [default: "DP-0,HDMI-0"]
    -h, --help                     Print help
    -V, --version                  Print version
```

### Examples

1. Basic usage with just API key:
```bash
healthy-monitor --api-key YOUR_API_KEY
```

2. Custom brightness and monitors:
```bash
healthy-monitor \
    --api-key YOUR_API_KEY \
    --min-brightness 0.4 \
    --monitors "HDMI-1,DP-1"
```

3. Custom color temperatures:
```bash
healthy-monitor \
    --api-key YOUR_API_KEY \
    --day-temp 7000 \
    --night-temp 2700
```

### Automatic Execution with Crontab

To run healthy-monitor automatically at regular intervals:

1. Find the path to the installed binary:
```bash
which healthy-monitor
```

2. Open your crontab configuration:
```bash
crontab -e
```

3. Add one of these example configurations:

```bash
# Run every 5 minutes
*/5 * * * * DISPLAY=:0 /path/to/healthy-monitor --api-key YOUR_API_KEY

# Run every 10 minutes during daytime (7 AM to 10 PM)
*/10 7-22 * * * DISPLAY=:0 /path/to/healthy-monitor --api-key YOUR_API_KEY

# Run every 30 minutes with custom settings
*/30 * * * * DISPLAY=:0 /path/to/healthy-monitor --api-key YOUR_API_KEY --min-brightness 0.4 --monitors "HDMI-1,DP-1"
```

Note: 
- Replace `/path/to/healthy-monitor` with the actual path from step 1
- Replace `YOUR_API_KEY` with your OpenWeather API key
- The `DISPLAY=:0` is required for X11 access
- Adjust the timing pattern (the five fields at the start) as needed:
  - `*/5` means "every 5 minutes"
  - `7-22` means "from 7 AM to 10 PM"
  - See `man 5 crontab` for more timing patterns

### Monitor Configuration

To find your monitor names, run:
```bash
xrandr --listmonitors
```

Then use these names in the `--monitors` option.

## How It Works

1. The application first attempts to use your webcam to measure ambient light.
2. If the webcam is unavailable, it falls back to using weather data:
   - Fetches your location using IP geolocation
   - Gets weather data from OpenWeather API
   - Calculates brightness based on time of day and cloud coverage
3. Adjusts monitor brightness and color temperature using `xrandr`
4. Color temperature transitions gradually from day to night

## License

MIT License 