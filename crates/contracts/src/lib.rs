pub mod gridforge {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/gridforge.v1.rs"));
    }
}

pub use gridforge::v1::Envelope;
