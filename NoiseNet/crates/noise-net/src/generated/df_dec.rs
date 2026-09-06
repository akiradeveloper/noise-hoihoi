// Generated from the official DeepFilterNet3 df_dec.onnx graph by Burn 0.22.0-pre.3.
extern crate alloc;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::PaddingConfig2d;
use burn::nn::conv::Conv2d;
use burn::nn::conv::Conv2dConfig;
use burn::prelude::*;
use burn::tensor::Bytes;
use burn::tensor::TensorData;
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;

#[derive(Module, Debug)]
pub struct Model {
    constant2: burn::module::Param<Tensor<3>>,
    constant3: burn::module::Param<Tensor<3>>,
    constant4: burn::module::Param<Tensor<3>>,
    gru1: burn::nn::gru::Gru,
    gru2: burn::nn::gru::Gru,
    conv2d1: Conv2d,
    conv2d2: Conv2d,
    linear1: Linear,
    #[module(skip)]
    device: Device,
}

impl Model {
    /// One streaming deep-filter decoder step with explicit convolution and GRU state.
    pub(crate) fn forward_stream(
        &self,
        embedding: Tensor<3>,
        c0_window: Tensor<4>,
        state0: Tensor<2>,
        state1: Tensor<2>,
    ) -> (Tensor<4>, Tensor<2>, Tensor<2>) {
        use crate::network::grouped_linear;

        let input = burn::tensor::activation::relu(grouped_linear(
            embedding.clone(),
            self.constant2.val(),
            8,
        ));
        let hidden0 = self.gru1.forward(input, Some(state0));
        let state0 = hidden0.clone().squeeze_dim(1);
        let hidden1 = self.gru2.forward(hidden0, Some(state1));
        let state1 = hidden1.clone().squeeze_dim(1);
        let hidden = hidden1.add(grouped_linear(embedding, self.constant3.val(), 16));

        let pathway =
            burn::tensor::activation::relu(self.conv2d2.forward(self.conv2d1.forward(c0_window)))
                .permute([0, 2, 3, 1]);
        let coefficients = grouped_linear(hidden, self.constant4.val(), 16)
            .tanh()
            .reshape([1, 1, 96, 10])
            .add(pathway);
        (coefficients, state0, state1)
    }
}

extern crate std;

impl Model {
    /// Load model weights from a burnpack file.
    pub fn from_file<P: AsRef<std::path::Path>>(file: P, device: &Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_file(&file);
        model.load_from(&mut store).unwrap_or_else(|e| {
            panic!(
                "Failed to load burnpack file {}: {e}",
                file.as_ref().display()
            )
        });
        model
    }

    /// Load model weights from in-memory bytes.
    ///
    /// The bytes must be the contents of a `.bpk` file.
    pub fn from_bytes(bytes: Bytes, device: &Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_bytes(Some(bytes));
        model
            .load_from(&mut store)
            .unwrap_or_else(|e| panic!("Failed to load burnpack bytes: {e}"));
        model
    }
}

impl Model {
    #[allow(unused_variables)]
    pub fn new(device: &Device) -> Self {
        let constant2: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([8, 64, 32], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [8, 64, 32].into(),
        );
        let constant3: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([16, 32, 16], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [16, 32, 16].into(),
        );
        let constant4: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([16, 16, 60], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [16, 16, 60].into(),
        );
        let gru1 = burn::nn::gru::GruConfig::new(256, 256, true)
            .with_reset_after(true)
            .init(device);
        let gru2 = burn::nn::gru::GruConfig::new(256, 256, true)
            .with_reset_after(true)
            .init(device);
        let conv2d1 = Conv2dConfig::new([64, 10], [5, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(2)
            .with_bias(false)
            .init(device);
        let conv2d2 = Conv2dConfig::new([10, 10], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let linear1 = LinearConfig::new(256, 1).with_bias(true).init(device);
        Self {
            constant2,
            constant3,
            constant4,
            gru1,
            gru2,
            conv2d1,
            conv2d2,
            linear1,
            device: device.clone(),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(&self, emb: Tensor<3>, c0: Tensor<4>) -> (Tensor<4>, Tensor<3>) {
        let constant2_out1 = self.constant2.val();
        let constant3_out1 = self.constant3.val();
        let constant4_out1 = self.constant4.val();
        let shape1_out1: [i64; 3] = {
            let axes = &emb.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather1_out1 = shape1_out1[0] as i64;
        let gather2_out1 = shape1_out1[1] as i64;
        let unsqueeze1_out1 = [gather1_out1 as i64];
        let unsqueeze2_out1 = [gather2_out1 as i64];
        let constant17_out1: [i64; 1] = [8i64];
        let constant18_out1: [i64; 1] = [64i64];
        let concat1_out1: [i64; 4usize] = [
            &unsqueeze1_out1[..],
            &unsqueeze2_out1[..],
            &constant17_out1[..],
            &constant18_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape1_out1 = emb.clone().reshape(concat1_out1);
        let einsum1_out1 = {
            let einsum_lhs = reshape1_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant2_out1;
            let einsum_lhs_shape = einsum_lhs.dims();
            let einsum_rhs_shape = einsum_rhs.dims();
            let einsum_lhs_3d: Tensor<3> = einsum_lhs.reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[1usize] * einsum_lhs_shape[2usize],
                einsum_lhs_shape[3usize],
            ]);
            let einsum_rhs_3d: Tensor<3> = einsum_rhs.reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[3usize],
                einsum_rhs_shape[2usize],
            ]);
            let einsum_result: Tensor<4> = einsum_lhs_3d.matmul(einsum_rhs_3d).reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[1usize],
                einsum_lhs_shape[2usize],
                einsum_rhs_shape[2usize],
            ]);
            einsum_result.permute([1usize, 2usize, 0usize, 3usize])
        };
        let shape3_out1: [i64; 4] = {
            let axes = &einsum1_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice1_out1: [i64; 2] = shape3_out1[0..2].try_into().unwrap();
        let constant22_out1: [i64; 1] = [-1i64];
        let concat2_out1: [i64; 3usize] = [&slice1_out1[..], &constant22_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape2_out1 = einsum1_out1.reshape(concat2_out1);
        let relu1_out1 = burn::tensor::activation::relu(reshape2_out1);
        let shape4_out1: [i64; 3] = {
            let axes = &relu1_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather3_out1 = shape4_out1[0] as i64;
        let constant24_out1: [i64; 1] = [2i64];
        let unsqueeze3_out1 = [gather3_out1 as i64];
        let constant25_out1: [i64; 1] = [256i64];
        let concat3_out1: [i64; 3usize] = [
            &constant24_out1[..],
            &unsqueeze3_out1[..],
            &constant25_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let constantofshape1_out1 = Tensor::<1>::from_data(
            burn::tensor::TensorData::from([0f32 as f64]),
            (&self.device, burn::tensor::DType::F32),
        )
        .reshape([1, 1, 1])
        .expand(concat3_out1);
        let transpose1_out1 = relu1_out1.permute([1, 0, 2]);
        let slice2_out1 = constantofshape1_out1.clone().slice(s![0..1, .., ..]);
        let (gru1_out1, gru1_out2) = {
            let gru_output = self.gru1.forward(
                transpose1_out1.swap_dims(0, 1),
                Some(slice2_out1.squeeze_dim(0)),
            );
            let batch_first_output = gru_output;
            (
                batch_first_output
                    .clone()
                    .swap_dims(0, 1)
                    .unsqueeze_dims::<4>(&[1]),
                {
                    let [_batch, seq_len, _hidden] = batch_first_output.dims();
                    let step = batch_first_output.clone().slice([
                        0.._batch,
                        (seq_len - 1)..seq_len,
                        0.._hidden,
                    ]);
                    step.squeeze_dim::<2>(1).unsqueeze_dims::<3>(&[0])
                },
            )
        };
        let squeeze1_out1 = gru1_out1.squeeze_dims::<3>(&[1]);
        let slice3_out1 = constantofshape1_out1.slice(s![1..2, .., ..]);
        let (gru2_out1, gru2_out2) = {
            let gru_output = self.gru2.forward(
                squeeze1_out1.swap_dims(0, 1),
                Some(slice3_out1.squeeze_dim(0)),
            );
            let batch_first_output = gru_output;
            (
                batch_first_output
                    .clone()
                    .swap_dims(0, 1)
                    .unsqueeze_dims::<4>(&[1]),
                {
                    let [_batch, seq_len, _hidden] = batch_first_output.dims();
                    let step = batch_first_output.clone().slice([
                        0.._batch,
                        (seq_len - 1)..seq_len,
                        0.._hidden,
                    ]);
                    step.squeeze_dim::<2>(1).unsqueeze_dims::<3>(&[0])
                },
            )
        };
        let squeeze2_out1 = gru2_out1.squeeze_dims::<3>(&[1]);
        let transpose2_out1 = squeeze2_out1.permute([1, 0, 2]);
        let constant32_out1: [i64; 1] = [16i64];
        let constant33_out1: [i64; 1] = [32i64];
        let concat4_out1: [i64; 4usize] = [
            &unsqueeze1_out1[..],
            &unsqueeze2_out1[..],
            &constant32_out1[..],
            &constant33_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape3_out1 = emb.reshape(concat4_out1);
        let einsum2_out1 = {
            let einsum_lhs = reshape3_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant3_out1;
            let einsum_lhs_shape = einsum_lhs.dims();
            let einsum_rhs_shape = einsum_rhs.dims();
            let einsum_lhs_3d: Tensor<3> = einsum_lhs.reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[1usize] * einsum_lhs_shape[2usize],
                einsum_lhs_shape[3usize],
            ]);
            let einsum_rhs_3d: Tensor<3> = einsum_rhs.reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[3usize],
                einsum_rhs_shape[2usize],
            ]);
            let einsum_result: Tensor<4> = einsum_lhs_3d.matmul(einsum_rhs_3d).reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[1usize],
                einsum_lhs_shape[2usize],
                einsum_rhs_shape[2usize],
            ]);
            einsum_result.permute([1usize, 2usize, 0usize, 3usize])
        };
        let shape5_out1: [i64; 4] = {
            let axes = &einsum2_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice4_out1: [i64; 2] = shape5_out1[0..2].try_into().unwrap();
        let constant37_out1: [i64; 1] = [-1i64];
        let concat5_out1: [i64; 3usize] = [&slice4_out1[..], &constant37_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape4_out1 = einsum2_out1.reshape(concat5_out1);
        let add1_out1 = transpose2_out1.add(reshape4_out1);
        let constant39_out1: [i64; 4] = [0i64, 0i64, 4i64, 0i64];
        let constantofshape2_out1: [i64; 4usize] = [0i64, 0i64, 0i64, 0i64];
        let concat6_out1: [i64; 8usize] = [&constant39_out1[..], &constantofshape2_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape5_out1 = {
            let shape_array = concat6_out1 as [i64; 8usize];
            Tensor::<1, Int>::from_data(
                TensorData::from(shape_array),
                (&self.device, burn::tensor::DType::I64),
            )
        }
        .reshape([-1, 2]);
        let slice5_out1 = {
            let slice_input = reshape5_out1;
            let slice_dims = slice_input.dims();
            let reverse_bounds = |dim: usize, start: i64, end: i64| -> core::ops::Range<usize> {
                if dim == 0 {
                    return 0..0;
                }
                let dim = dim as i64;
                let start = if start < 0 {
                    start.saturating_add(dim)
                } else {
                    start
                };
                let end = if end < 0 {
                    end.saturating_add(dim)
                } else {
                    end
                };
                let hi = start.clamp(-1, dim - 1) + 1;
                let lo = (end.clamp(-1, dim - 1) + 1).min(hi);
                (lo as usize)..(hi as usize)
            };
            slice_input.slice(s![
                reverse_bounds(slice_dims[0], - 1i64, - 9223372036854775807i64);
                - 1, ..
            ])
        };
        let transpose3_out1 = slice5_out1.permute([1, 0]);
        let reshape6_out1 = transpose3_out1.reshape([-1]);
        let pad1_out1 = c0.pad(
            {
                let __raw: alloc::vec::Vec<i64> = reshape6_out1
                    .to_data()
                    .convert::<i64>()
                    .into_vec::<i64>()
                    .unwrap();
                assert_eq!(
                    __raw.len(),
                    8usize,
                    "Pad: runtime pads length mismatch (expected {}, got {})",
                    8usize,
                    __raw.len(),
                );
                [
                    (
                        usize::try_from(__raw[0usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[0usize], 0usize
                            )
                        }),
                        usize::try_from(__raw[4usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[4usize], 4usize
                            )
                        }),
                    ),
                    (
                        usize::try_from(__raw[1usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[1usize], 1usize
                            )
                        }),
                        usize::try_from(__raw[5usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[5usize], 5usize
                            )
                        }),
                    ),
                    (
                        usize::try_from(__raw[2usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[2usize], 2usize
                            )
                        }),
                        usize::try_from(__raw[6usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[6usize], 6usize
                            )
                        }),
                    ),
                    (
                        usize::try_from(__raw[3usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[3usize], 3usize
                            )
                        }),
                        usize::try_from(__raw[7usize]).unwrap_or_else(|_| {
                            panic!(
                                "Pad: negative pad value {} at index {}",
                                __raw[7usize], 7usize
                            )
                        }),
                    ),
                ]
            },
            burn::tensor::ops::PadMode::Constant(0f32),
        );
        let conv2d1_out1 = self.conv2d1.forward(pad1_out1);
        let conv2d2_out1 = self.conv2d2.forward(conv2d1_out1);
        let relu2_out1 = burn::tensor::activation::relu(conv2d2_out1);
        let transpose4_out1 = relu2_out1.permute([0, 2, 3, 1]);
        let linear1_out1 = self.linear1.forward(add1_out1.clone());
        let sigmoid1_out1 = burn::tensor::activation::sigmoid(linear1_out1);
        let shape6_out1: [i64; 3] = {
            let axes = &add1_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather4_out1 = shape6_out1[0] as i64;
        let gather5_out1 = shape6_out1[1] as i64;
        let unsqueeze6_out1 = [gather4_out1 as i64];
        let unsqueeze7_out1 = [gather5_out1 as i64];
        let constant49_out1: [i64; 1] = [16i64];
        let constant50_out1: [i64; 1] = [16i64];
        let concat7_out1: [i64; 4usize] = [
            &unsqueeze6_out1[..],
            &unsqueeze7_out1[..],
            &constant49_out1[..],
            &constant50_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape7_out1 = add1_out1.reshape(concat7_out1);
        let einsum3_out1 = {
            let einsum_lhs = reshape7_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant4_out1;
            let einsum_lhs_shape = einsum_lhs.dims();
            let einsum_rhs_shape = einsum_rhs.dims();
            let einsum_lhs_3d: Tensor<3> = einsum_lhs.reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[1usize] * einsum_lhs_shape[2usize],
                einsum_lhs_shape[3usize],
            ]);
            let einsum_rhs_3d: Tensor<3> = einsum_rhs.reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[3usize],
                einsum_rhs_shape[2usize],
            ]);
            let einsum_result: Tensor<4> = einsum_lhs_3d.matmul(einsum_rhs_3d).reshape([
                einsum_lhs_shape[0usize],
                einsum_lhs_shape[1usize],
                einsum_lhs_shape[2usize],
                einsum_rhs_shape[2usize],
            ]);
            einsum_result.permute([1usize, 2usize, 0usize, 3usize])
        };
        let shape8_out1: [i64; 4] = {
            let axes = &einsum3_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice6_out1: [i64; 2] = shape8_out1[0..2].try_into().unwrap();
        let constant54_out1: [i64; 1] = [-1i64];
        let concat8_out1: [i64; 3usize] = [&slice6_out1[..], &constant54_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape8_out1 = einsum3_out1.reshape(concat8_out1);
        let tanh1_out1 = reshape8_out1.tanh();
        let constant55_out1: [i64; 1] = [96i64];
        let constant56_out1: [i64; 1] = [10i64];
        let concat9_out1: [i64; 4usize] = [
            &unsqueeze1_out1[..],
            &unsqueeze2_out1[..],
            &constant55_out1[..],
            &constant56_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape9_out1 = tanh1_out1.reshape(concat9_out1);
        let add2_out1 = reshape9_out1.add(transpose4_out1);
        (add2_out1, sigmoid1_out1)
    }
}
