/* Copyright (c) 2023 Nuand LLC
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use libbladeRF::*;
use std::ffi::CStr;
use std::ptr::*;

struct test_params {
    num_samples: i64,
    samp_rate: bladerf_sample_rate,
    gain: i32,
    frequency: u64,
    ch: bladerf_channel,
    sync_format: bladerf_format,
    direction: bladerf_direction,
    num_iterations: i32,
}

fn init(dev: *mut bladerf, config: &test_params) -> Result<i32, String> {
    let layout: bladerf_channel_layout;

    if config.direction == bladerf_direction_BLADERF_TX {
        layout = bladerf_channel_layout_BLADERF_TX_X1;
    } else if config.direction == bladerf_direction_BLADERF_RX {
        layout = bladerf_channel_layout_BLADERF_RX_X1;
    } else {
        return Err("Invalid direction".to_string());
    }

    unsafe {
        match bladerf_set_frequency(dev, config.ch, config.frequency) {
            0 => println!("Frequency set to {} Hz", config.frequency),
            _ => return Err("Failed to set frequency".to_string()),
        }

        match bladerf_set_sample_rate(dev, config.ch, config.samp_rate, null_mut()) {
            0 => println!("Sample rate set to {} Hz", config.samp_rate),
            _ => return Err("Failed to set sample rate".to_string()),
        }

        if config.direction == bladerf_direction_BLADERF_RX {
            match bladerf_set_gain_mode(dev, config.ch, bladerf_gain_mode_BLADERF_GAIN_MGC) {
                0 => println!("Gain mode set to manual"),
                _ => return Err("Failed to set gain mode".to_string()),
            }
        }

        match bladerf_set_gain(dev, config.ch, config.gain) {
            0 => println!("Gain set to {} dB", config.gain),
            _ => return Err("Failed to set gain".to_string()),
        }

        match bladerf_sync_config(
            dev,
            layout,
            config.sync_format,
            512,
            32 * 1024 as u32,
            16,
            1000,
        ) {
            0 => println!("Sync config set"),
            _ => return Err("Failed to sync config".to_string()),
        }

        match bladerf_enable_module(dev, config.ch, true) {
            0 => println!("Module enabled"),
            _ => return Err("Failed to enable module".to_string()),
        }

        return Ok(0);
    }
}

fn stream_tx(dev: *mut bladerf, config: &test_params) -> Result<i32, String> {
    let mut samples: Vec<i16> = vec![0; 2 * config.num_samples as usize];

    // Transmit CW at Fc - Fs/4
    let i_mask: Vec<i16> = [0, 1, 0, -1].to_vec();
    let q_mask: Vec<i16> = [1, 0, -1, 0].to_vec();

    for i in 0..config.num_samples {
        samples[2 * i as usize] = 2047 * i_mask[(i % 4) as usize];
        samples[2 * i as usize + 1] = 2047 * q_mask[(i % 4) as usize];
    }

    let mut metadata: bladerf_metadata = bladerf_metadata {
        timestamp: 0,
        status: 0,
        flags: BLADERF_META_FLAG_TX_BURST_START
            | BLADERF_META_FLAG_TX_BURST_END,
        actual_count: 0,
        reserved: [0; 32],
    };
    unsafe {
        match bladerf_get_timestamp(dev, bladerf_direction_BLADERF_TX, &mut metadata.timestamp)
        {
            0 => println!("Timestamp from get: {}", metadata.timestamp),
            _ => return Err("Failed to get timestamp".to_string()),
        }
    }

    unsafe {
        for i in 0..config.num_iterations {
            match bladerf_sync_tx(
                dev,
                samples.as_mut_ptr() as *mut _,
                config.num_samples as u32,
                &mut metadata,
                1000,
            ) {
                0 => {println!("Timestamp: {}", metadata.timestamp)}
                _ => return Err("Failed to sync tx".to_string()),
            }
            metadata.timestamp += config.samp_rate as u64;


        }

        return Ok(0);
    }
}

fn calculate_avg_power(samples: &[i16]) -> f64 {
    let mut total_power = 0.0;

    for chunk in samples.chunks(2) {
        if let [i, q] = *chunk {
            let i = i as f64;
            let q = q as f64;
            total_power += i.powi(2) + q.powi(2);
        }
    }

    total_power /= (samples.len() / 2) as f64; // Divide by number of I/Q pairs
    total_power /= 2047_f64.powf(2.0);
    total_power = 10.0 * total_power.log10();

    let avg_power = if !samples.is_empty() {
        total_power
    } else {
        0.0
    };

    return avg_power;
}

fn stream_rx(dev: *mut bladerf, config: &test_params) -> Result<i32, String> {
    let mut samples: Vec<i16> = vec![0; 2 * config.num_samples as usize];
    let mut avg_power;
    let mut metadata: bladerf_metadata = bladerf_metadata {
        timestamp: 0,
        status: 0,
        flags: 0,
        actual_count: 0,
        reserved: [0; 32],
    };

    unsafe {
        let mut t = 0;
        match bladerf_get_timestamp(dev, bladerf_direction_BLADERF_RX, &mut metadata.timestamp) {
            0 => {
                println!("Timestamp from get_timestamp: {:?}", metadata.timestamp);
            }
            _ => return Err("Failed to get timestamp".to_string().into()),
        };
        println!("metadata.timestamp = {}", metadata.timestamp);
        let sync_interval = u64::from(config.samp_rate) * 5;
        let mut next_sync_timestamp = metadata.timestamp;
        let delay = u64::from(config.samp_rate) * 300 / 1000;

        loop {
            if next_sync_timestamp - metadata.timestamp <= delay + u64::from(config.samp_rate) {
                metadata.timestamp = next_sync_timestamp;
            } else {
                metadata.timestamp += delay;
            }

            match bladerf_sync_rx(
                dev,
                samples.as_mut_ptr() as *mut _,
                config.num_samples as u32,
                &mut metadata,
                10000) // Timeout in milliseconds
        {
                0 => {
                    if metadata.status & BLADERF_META_STATUS_OVERRUN != 0 {
                        println!("Overrun detected. {} valid samples were read.", metadata.actual_count);
                    } else {
                        avg_power = calculate_avg_power(&samples);
                    println!("RX'd {} samples at t={}, Avg. Power = {:.2}dBFS",
                            metadata.actual_count, metadata.timestamp, avg_power);
                    }
                }
                _ => {
                eprintln!("RX \"now\" failed");
            }
            }
            match next_sync_timestamp == metadata.timestamp {
                true => {
                    println!("sync metadata.timestamp = {}", metadata.timestamp);
                    next_sync_timestamp = metadata.timestamp + sync_interval;
                }
                _ => {
                    println!("metadata.timestamp = {}", metadata.timestamp);
                }
            }

            //  metadata.timestamp += config.num_samples + ts_inc_1ms;
            // metadata.timestamp += config.num_samples as u64;
        }

        return Ok(0);
    }
}

fn main() {
    let tx_config: test_params = test_params {
        direction: bladerf_direction_BLADERF_TX,
        samp_rate: 30.72e6 as u32,
        num_samples: 20e4 as i64,
        gain: 60,
        frequency: 900e6 as u64,
        ch: 1, // TX0
        sync_format: bladerf_format_BLADERF_FORMAT_SC16_Q11_META,
        num_iterations: 80,
    };

    let rx_config: test_params = test_params {
        direction: bladerf_direction_BLADERF_RX,
        samp_rate: 30.72e6 as u32,
        num_samples: 6144000 as i64,
        gain: 50,
        frequency: 900e6 as u64,
        ch: 0, // RX0
        sync_format: bladerf_format_BLADERF_FORMAT_SC16_Q11_META,
        num_iterations: 5,
    };

    let mut dev: *mut bladerf = std::ptr::null_mut();
    let mut dev_info: bladerf_devinfo = bladerf_devinfo {
        usb_bus: 0,
        usb_addr: 0,
        instance: 0,
        serial: [0; 33],
        product: [0; 33],
        manufacturer: [0; 33],
        backend: bladerf_backend_BLADERF_BACKEND_ANY,
    };

    let mut version = bladerf_version {
        major: 0,
        minor: 0,
        patch: 0,
        describe: std::ptr::null(),
    };

    unsafe {
        bladerf_log_set_verbosity(bladerf_log_level_BLADERF_LOG_LEVEL_DEBUG);

        bladerf_version(&mut version as *mut _);
        let describe_cstr = CStr::from_ptr(version.describe);
        let describe_str = describe_cstr.to_str().unwrap();
        println!(
            "libbladeRF version: {}.{}.{} - {}",
            version.major, version.minor, version.patch, describe_str
        );

        bladerf_init_devinfo(&mut dev_info);

        match bladerf_open_with_devinfo(&mut dev, &mut dev_info) {
            0 => println!("Device opened"),
            _ => println!("Failed to open device"),
        }
        for i in 0..2 {
            match init(dev, &tx_config) {
                Ok(_) => println!("Device initialized"),
                Err(e) => println!("Error: {}", e),
            }
    
            match stream_tx(dev, &tx_config) {
                Ok(_) => println!("Streamed {}M samples", tx_config.num_samples * tx_config.num_iterations as i64 / 1e6 as i64),
                Err(e) => println!("Error: {}", e),
            }
    
            match bladerf_enable_module(dev, tx_config.ch, false) {
                0 => println!("TX module disabled"),
                _ => println!("Failed to disable module"),
            }
        }

        // match init(dev, &rx_config) {
        //     Ok(_) => println!("Device initialized"),
        //     Err(e) => println!("Error: {}", e),
        // }

        // match stream_rx(dev, &rx_config) {
        //     Ok(_) => println!(
        //         "Streamed {}M samples",
        //         tx_config.num_samples * tx_config.num_iterations as i64 / 1e6 as i64
        //     ),
        //     Err(e) => println!("Error: {}", e),
        // }

        // match bladerf_enable_module(dev, rx_config.ch, false) {
        //     0 => println!("RX module disabled"),
        //     _ => println!("Failed to disable module"),
        // }

        bladerf_close(dev);
    }
}
