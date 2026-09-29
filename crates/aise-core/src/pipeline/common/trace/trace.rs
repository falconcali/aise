use crate::trace::Trace;
use crate::trace::Observation;

fn begin_observation(trace: &Trace, stage: &str) -> Observation {
    trace.observation(stage)
}

fn finish_observation(observation: Observation) {
    // TODO
}