pub struct State {
    acknowledged: u32,
    client_sequence: u32,
    /// The last remote reliable state
    client_reliable: u8,
    server_sequence: u32,
    /// Our current reliable state
    server_reliable: u8,
}

impl State {
    pub fn flip_rel(&mut self, n: usize) {
        self.server_reliable ^= 1 << n;
    }

    pub fn get_rel(&self) -> u8 {
        self.client_reliable
    }

    pub fn new() -> Self {
        Self {
            acknowledged: 0,
            client_sequence: 1,
            client_reliable: 0,
            server_sequence: 1,
            server_reliable: 0,
        }
    }

    pub fn set_rel(&mut self, rel: u8) {
        self.client_reliable = rel;
    }

    pub fn rel(&self) -> u8 {
        self.server_reliable
    }
}
