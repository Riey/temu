pub struct ScrollState {
    pub top: u32,
    pub max: u32,
    pub page_size: u32,
}

// struct ScrollCalcResult {
//     top: f32,
//     bottom: f32,
// }

impl ScrollState {
    pub fn new() -> Self {
        Self {
            top: 0,
            max: 1,
            page_size: 1,
        }
    }

    // pub fn calculate(&self) -> ScrollCalcResult {
    //     match self.max.checked_sub(self.top) {
    //         None => ScrollCalcResult::FULL,
    //         Some(left) => ScrollCalcResult {
    //             top: self.top as f32 / self.max as f32,
    //             bottom: left as f32 / self.max as f32,
    //         },
    //     }
    // }
}

// impl ScrollCalcResult {
//     /// Can display all lines
//     const FULL: Self = ScrollCalcResult {
//         top: 0.0,
//         bottom: 1.0,
//     };
// }