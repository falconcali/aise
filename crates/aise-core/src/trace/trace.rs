use crate::trace::observation::Observation;

pub struct Trace {
}

impl Trace {
    pub fn new() -> Self {
        Trace {}
    }

    pub fn observation(&self, stage: &str) -> Observation {
        Observation::new(stage)
    }
}