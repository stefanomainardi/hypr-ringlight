//! Camera detection for video call notifications
//!
//! Monitors video devices (/dev/video*) to detect when a camera becomes active.
//! Sends a desktop notification when the camera starts being used.

use notify_rust::Notification;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Check if any video device is currently in use
fn is_camera_in_use() -> bool {
    // Find all video devices
    let video_devices: Vec<_> = fs::read_dir("/dev")
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("video")
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    
    // Check if any device is being used via fuser
    for device in video_devices {
        let output = Command::new("fuser")
            .arg(device.to_string_lossy().as_ref())
            .output();
        
        if let Ok(output) = output {
            // fuser returns non-empty stdout if the file is in use
            if !output.stdout.is_empty() || output.status.success() {
                return true;
            }
        }
    }
    
    false
}

/// Send a notification about the ring light
fn send_notification() {
    let _ = Notification::new()
        .summary("Camera Active")
        .body("Your webcam is now active. Consider enabling the ring light for better lighting!")
        .icon("camera-web")
        .hint(notify_rust::Hint::Urgency(notify_rust::Urgency::Low))
        .hint(notify_rust::Hint::Category("device".to_string()))
        .timeout(10000) // 10 seconds
        .show();
}

/// Start the camera monitoring thread
/// 
/// This runs in the background and checks periodically if the camera becomes active.
/// When the camera is activated, it sends a notification to remind the user about the ring light.
pub fn start_camera_monitor(ring_visible: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut was_in_use = false;
        
        loop {
            let is_in_use = is_camera_in_use();
            
            // Camera just became active
            if is_in_use && !was_in_use {
                // Only notify if ring light is not currently visible
                if !ring_visible.load(Ordering::Relaxed) {
                    send_notification();
                }
            }
            
            was_in_use = is_in_use;
            
            // Check every 5 seconds (balance between responsiveness and CPU usage)
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}
