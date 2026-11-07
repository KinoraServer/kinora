//! Pure logic for the media server.
//!
//! Nothing in this crate touches the disk, the network, or spawns processes.
//! That keeps every function here fast and trivial to unit test: give it
//! plain data, check the plain data it returns.

pub mod ffmpeg;
pub mod hls;
pub mod media;
pub mod naming;
pub mod playback;
pub mod roles;
pub mod tracks;
