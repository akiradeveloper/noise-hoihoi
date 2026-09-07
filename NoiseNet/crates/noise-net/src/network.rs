use burn::{
    prelude::*,
    tensor::{Bytes, TensorData},
};

use crate::{DF_BINS, ERB_BANDS};

use super::generated::{df_dec, enc, erb_dec};

const HIDDEN_SIZE: usize = 256;
const ENCODER_CONTEXT: usize = 3;
const DF_PATH_CONTEXT: usize = 5;

#[derive(Clone, Copy)]
pub(crate) enum InferenceMode {
    /// Both decoders advance on every frame, including pauses and quiet speech.
    Continuous,
    /// Preserve the pinned upstream runtime for reference comparisons.
    Reference,
}

pub(crate) struct NetworkOutput {
    pub(crate) mask: Option<Vec<f32>>,
    pub(crate) coefficients: Option<Vec<f32>>,
    pub(crate) local_snr_db: f32,
}

pub(crate) struct StreamingNetwork {
    encoder: enc::Model,
    erb_decoder: erb_dec::Model,
    df_decoder: df_dec::Model,
    device: Device,
    encoder_state: Tensor<2>,
    erb_states: [Tensor<2>; 2],
    df_states: [Tensor<2>; 2],
    erb_history: Vec<[f32; ERB_BANDS]>,
    spec_history: Vec<[[f32; DF_BINS]; 2]>,
    c0_history: Vec<Tensor<4>>,
}

impl StreamingNetwork {
    pub(crate) fn new(device: Device) -> Self {
        let encoder = enc::Model::from_bytes(
            Bytes::from_bytes_vec(include_bytes!("../model/enc.bpk").to_vec()),
            &device,
        );
        let erb_decoder = erb_dec::Model::from_bytes(
            Bytes::from_bytes_vec(include_bytes!("../model/erb_dec.bpk").to_vec()),
            &device,
        );
        let df_decoder = df_dec::Model::from_bytes(
            Bytes::from_bytes_vec(include_bytes!("../model/df_dec.bpk").to_vec()),
            &device,
        );
        let mut network = Self {
            encoder,
            erb_decoder,
            df_decoder,
            encoder_state: Tensor::zeros([1, HIDDEN_SIZE], &device),
            erb_states: std::array::from_fn(|_| Tensor::zeros([1, HIDDEN_SIZE], &device)),
            df_states: std::array::from_fn(|_| Tensor::zeros([1, HIDDEN_SIZE], &device)),
            erb_history: Vec::new(),
            spec_history: Vec::new(),
            c0_history: Vec::new(),
            device,
        };
        network.reset();
        network
    }

    pub(crate) fn reset(&mut self) {
        self.encoder_state = Tensor::zeros([1, HIDDEN_SIZE], &self.device);
        self.erb_states = std::array::from_fn(|_| Tensor::zeros([1, HIDDEN_SIZE], &self.device));
        self.df_states = std::array::from_fn(|_| Tensor::zeros([1, HIDDEN_SIZE], &self.device));
        self.erb_history = vec![[0.0; ERB_BANDS]; ENCODER_CONTEXT];
        self.spec_history = vec![[[0.0; DF_BINS]; 2]; ENCODER_CONTEXT];
        self.c0_history = (0..DF_PATH_CONTEXT)
            .map(|_| Tensor::zeros([1, 64, 1, DF_BINS], &self.device))
            .collect();
    }

    pub(crate) fn infer(
        &mut self,
        erb: [f32; ERB_BANDS],
        complex_features: &[[f32; DF_BINS]; 2],
        mode: InferenceMode,
    ) -> NetworkOutput {
        self.erb_history.rotate_left(1);
        self.erb_history[ENCODER_CONTEXT - 1] = erb;
        self.spec_history.rotate_left(1);
        self.spec_history[ENCODER_CONTEXT - 1] = *complex_features;

        let erb_data = self
            .erb_history
            .iter()
            .flat_map(|frame| frame.iter().copied())
            .collect::<Vec<_>>();
        let erb_input = Tensor::<4>::from_data(
            TensorData::new(erb_data, [1, 1, ENCODER_CONTEXT, ERB_BANDS]),
            &self.device,
        );
        let spec_data = (0..2)
            .flat_map(|component| {
                self.spec_history
                    .iter()
                    .flat_map(move |frame| frame[component].iter().copied())
            })
            .collect::<Vec<_>>();
        let spec_input = Tensor::<4>::from_data(
            TensorData::new(spec_data, [1, 2, ENCODER_CONTEXT, DF_BINS]),
            &self.device,
        );

        let encoded =
            self.encoder
                .forward_stream(erb_input, spec_input, self.encoder_state.clone());
        self.encoder_state = encoded.state;
        let local_snr_db = encoded
            .local_snr_db
            .to_data()
            .try_to_vec::<f32>()
            .expect("encoder local SNR must be f32")[0];

        // Skipping a decoder also freezes its recurrent state and DF context.
        // The product keeps those histories continuous and lets the learned
        // filter estimate attenuation, without a global noise-only mute.
        let (apply_mask, zero_mask, apply_df) = match mode {
            InferenceMode::Continuous => (true, false, true),
            InferenceMode::Reference => processing_stages(local_snr_db),
        };
        let mask = if apply_mask {
            let (mask, state0, state1) = self.erb_decoder.forward_stream(
                encoded.embedding.clone(),
                encoded.e3,
                encoded.e2,
                encoded.e1,
                encoded.e0,
                self.erb_states[0].clone(),
                self.erb_states[1].clone(),
            );
            self.erb_states = [state0, state1];
            Some(
                mask.to_data()
                    .try_to_vec::<f32>()
                    .expect("ERB mask must be f32"),
            )
        } else if zero_mask {
            Some(vec![0.0; ERB_BANDS])
        } else {
            None
        };

        let coefficients = if apply_df {
            self.c0_history.rotate_left(1);
            self.c0_history[DF_PATH_CONTEXT - 1] = encoded.c0;
            let c0_window = Tensor::cat(self.c0_history.iter().map(Clone::clone).collect(), 2);
            let (coefficients, state0, state1) = self.df_decoder.forward_stream(
                encoded.embedding,
                c0_window,
                self.df_states[0].clone(),
                self.df_states[1].clone(),
            );
            self.df_states = [state0, state1];
            Some(
                coefficients
                    .to_data()
                    .try_to_vec::<f32>()
                    .expect("DF coefficients must be f32"),
            )
        } else {
            None
        };

        NetworkOutput {
            mask,
            coefficients,
            local_snr_db,
        }
    }
}

fn processing_stages(local_snr_db: f32) -> (bool, bool, bool) {
    if local_snr_db < crate::NOISE_ONLY_THRESHOLD_DB {
        (false, true, false)
    } else if local_snr_db > 30.0 {
        (false, false, false)
    } else if local_snr_db > 20.0 {
        (true, false, false)
    } else {
        (true, false, true)
    }
}

pub(crate) fn grouped_linear(input: Tensor<3>, weight: Tensor<3>, groups: usize) -> Tensor<3> {
    let [batch, time, input_size] = input.dims();
    let [weight_groups, input_per_group, output_per_group] = weight.dims();
    assert_eq!(groups, weight_groups);
    assert_eq!(input_size, groups * input_per_group);
    let lhs = input
        .reshape([batch, time, groups, input_per_group])
        .permute([2, 0, 1, 3])
        .reshape([groups, batch * time, input_per_group]);
    lhs.matmul(weight)
        .reshape([groups, batch, time, output_per_group])
        .permute([1, 2, 0, 3])
        .reshape([batch, time, groups * output_per_group])
}

#[cfg(test)]
mod tests {
    use super::processing_stages;

    #[test]
    fn stage_thresholds_match_the_official_runtime() {
        assert_eq!(processing_stages(-10.1), (false, true, false));
        assert_eq!(processing_stages(-10.0), (true, false, true));
        assert_eq!(processing_stages(20.0), (true, false, true));
        assert_eq!(processing_stages(20.1), (true, false, false));
        assert_eq!(processing_stages(30.1), (false, false, false));
    }
}
