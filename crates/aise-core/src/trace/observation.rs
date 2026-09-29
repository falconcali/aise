pub struct Observation {
    pub stage: String,
}

impl Observation {
    pub fn new(stage: &str) -> Self {
        Observation {
            stage: stage.to_string(),
        }
    }
}