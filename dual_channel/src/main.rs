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

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BladerfChannel {
    RX1 = 0,
    TX1 = 1,
    RX2 = 2,
    TX2 = 3,
}

struct test_params {
    num_samples: i64,
    samp_rate: bladerf_sample_rate,
    gain: i32,
    frequency: u64,
    sync_format: bladerf_format,
    channels: Vec<BladerfChannel>,
    flags: u32,
    num_buffers: u32,
    buffer_size: u32,
}

fn init(dev: *mut bladerf, config: &test_params) -> Result<i32, String> {
    let layout= bladerf_channel_layout_BLADERF_RX_X2;
    unsafe {
        for channel in &config.channels {
            let channel_id_i32 = *channel as i32;
            match bladerf_set_frequency(dev, channel_id_i32, config.frequency) {
                0 => println!("Frequency set to {} Hz", config.frequency),
                _ => return Err("Failed to set frequency".to_string()),
            }

            match bladerf_set_sample_rate(dev, channel_id_i32, config.samp_rate, null_mut()) {
                0 => println!("Sample rate set to {} Hz", config.samp_rate),
                _ => return Err("Failed to set sample rate".to_string()),
            }

            match bladerf_set_gain_mode(dev, channel_id_i32, bladerf_gain_mode_BLADERF_GAIN_MGC) {
                0 => println!("Gain mode set to manual"),
                _ => return Err("Failed to set gain mode".to_string()),
            }

            match bladerf_set_gain(dev, channel_id_i32, config.gain) {
                0 => println!("Gain set to {} dB", config.gain),
                _ => return Err("Failed to set gain".to_string()),
            }
        }
        match bladerf_sync_config(
            dev,
            layout,
            config.sync_format,
            config.num_buffers,
            config.buffer_size as u32,
            16,
            1000,
        ) {
            0 => println!("Sync config set"),
            _ => return Err("Failed to sync config".to_string()),
        }
        for channel in &config.channels {
            match bladerf_enable_module(dev, *channel as i32, true) {
                0 => println!("Module enabled"),
                _ => return Err("Failed to enable module".to_string()),
            }
        }
        return Ok(0);
    }
}

fn main() {
    let channels = vec![BladerfChannel::RX1, BladerfChannel::RX2];
    // Example code params -- Works
    // let rx_config: test_params = test_params {
    //     samp_rate: 1e6 as u32,
    //     num_samples: 2 * 2048 as i64,
    //     gain: 50,
    //     frequency: 2400e6 as u64,
    //     sync_format: bladerf_format_BLADERF_FORMAT_SC16_Q11_META,
    //     channels: channels.clone(),
    //     flags: BLADERF_META_FLAG_RX_NOW,
    //     num_buffers: 32,
    //     buffer_size: 8192,
    // };

    // Example code params without NOW flag -- Sample discontinuity
    // let rx_config: test_params = test_params {
    //     samp_rate: 1e6 as u32,
    //     num_samples: 2 * 2048 as i64,
    //     gain: 50,
    //     frequency: 2400e6 as u64,
    //     sync_format: bladerf_format_BLADERF_FORMAT_SC16_Q11_META,
    //     channels: channels.clone(),
    //     flags: 0,
    //     num_buffers: 32,
    //     buffer_size: 8192,
    // };

    // Our program code params -- Sample discontinuity
    let rx_config: test_params = test_params {
        samp_rate: 30.72e6 as u32,
        num_samples: 6144000 as i64,
        gain: 50,
        frequency: 2400e6 as u64,
        sync_format: bladerf_format_BLADERF_FORMAT_SC16_Q11_META,
        channels: channels.clone(),
        flags: 0, // doesn't work with BLADERF_META_FLAG_RX_NOW Either
        num_buffers: 512,
        buffer_size: 32 * 1024_u32,
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

        match init(dev, &rx_config) {
            Ok(_) => println!("Device initialized"),
            Err(e) => println!("Error: {}", e),
        }
    
        let mut metadata: bladerf_metadata = bladerf_metadata {
            timestamp: 0,
            status: 0,
            flags: rx_config.flags,
            actual_count: 0,
            reserved: [0; 32],
        };
        let mut rx_samples: Vec<i16> = vec![0; 2 * rx_config.num_samples as usize];
        match bladerf_get_timestamp(dev, bladerf_direction_BLADERF_RX, &mut metadata.timestamp)
        {
            0 => println!("Timestamp from get: {}", metadata.timestamp),
            _ => println!("Failed to get timestamp"),
        }
        let delay = u64::from(rx_config.samp_rate) * 250 / 1000;
        metadata.timestamp += delay;
        for _i in 0..20 {
            match bladerf_sync_rx(
                dev,
                rx_samples.as_mut_ptr() as *mut _,
                rx_config.num_samples as u32,
                &mut metadata,
                10000) // Timeout in milliseconds
            {
                0 => {
                    if metadata.status & BLADERF_META_STATUS_OVERRUN != 0 {
                        println!("Overrun detected. {} valid samples were read.", metadata.actual_count);
                    } else {
                    println!("RX'd {} samples at t={}",
                            metadata.actual_count, metadata.timestamp);
                    }
                }
                _ => {
                eprintln!("RX \"now\" failed");
                }
            }
            metadata.timestamp += delay;
        }
        for channel in channels {
            if bladerf_enable_module(dev, channel as i32, false) != 0 {
                println!("Failed to disable RX module for channel {:?}", channel);
            }
        }
        bladerf_close(dev);
    }
}
