// Generated from the official DeepFilterNet3 enc.onnx graph by Burn 0.22.0-pre.3.
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
    constant6: burn::module::Param<Tensor<3>>,
    constant7: burn::module::Param<Tensor<3>>,
    constant8: burn::module::Param<Tensor<3>>,
    conv2d1: Conv2d,
    conv2d2: Conv2d,
    conv2d3: Conv2d,
    conv2d4: Conv2d,
    conv2d5: Conv2d,
    conv2d6: Conv2d,
    conv2d7: Conv2d,
    conv2d8: Conv2d,
    conv2d9: Conv2d,
    conv2d10: Conv2d,
    conv2d11: Conv2d,
    gru1: burn::nn::gru::Gru,
    linear1: Linear,
    constant79: burn::module::Param<Tensor<1>>,
    constant80: burn::module::Param<Tensor<1>>,
    #[module(skip)]
    device: Device,
}

pub(crate) struct StreamOutput {
    pub(crate) e0: Tensor<4>,
    pub(crate) e1: Tensor<4>,
    pub(crate) e2: Tensor<4>,
    pub(crate) e3: Tensor<4>,
    pub(crate) embedding: Tensor<3>,
    pub(crate) c0: Tensor<4>,
    pub(crate) local_snr_db: Tensor<3>,
    pub(crate) state: Tensor<2>,
}

impl Model {
    /// One streaming encoder step with explicit convolution and GRU state.
    pub(crate) fn forward_stream(
        &self,
        feat_erb: Tensor<4>,
        feat_spec: Tensor<4>,
        state: Tensor<2>,
    ) -> StreamOutput {
        use crate::network::grouped_linear;

        let e0 = burn::tensor::activation::relu(self.conv2d1.forward(feat_erb));
        let e1 =
            burn::tensor::activation::relu(self.conv2d3.forward(self.conv2d2.forward(e0.clone())));
        let e2 =
            burn::tensor::activation::relu(self.conv2d5.forward(self.conv2d4.forward(e1.clone())));
        let e3 =
            burn::tensor::activation::relu(self.conv2d7.forward(self.conv2d6.forward(e2.clone())));
        let c0 =
            burn::tensor::activation::relu(self.conv2d9.forward(self.conv2d8.forward(feat_spec)));
        let c1 = burn::tensor::activation::relu(
            self.conv2d11.forward(self.conv2d10.forward(c0.clone())),
        );

        let c_embedding = grouped_linear(
            c1.permute([0, 2, 3, 1]).reshape([1, 1, 3_072]),
            self.constant6.val(),
            32,
        );
        let embedding = e3
            .clone()
            .permute([0, 2, 3, 1])
            .reshape([1, 1, 512])
            .add(burn::tensor::activation::relu(c_embedding));
        let gru_input =
            burn::tensor::activation::relu(grouped_linear(embedding, self.constant7.val(), 16));
        let gru_output = self.gru1.forward(gru_input, Some(state));
        let state = gru_output.clone().squeeze_dim(1);
        let embedding =
            burn::tensor::activation::relu(grouped_linear(gru_output, self.constant8.val(), 16));
        let local_snr_db =
            burn::tensor::activation::sigmoid(self.linear1.forward(embedding.clone()))
                .mul_scalar(50.0)
                .sub_scalar(15.0);

        StreamOutput {
            e0,
            e1,
            e2,
            e3,
            embedding,
            c0,
            local_snr_db,
            state,
        }
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
        let constant6: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([32, 96, 16], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [32, 96, 16].into(),
        );
        let constant7: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([16, 32, 16], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [16, 32, 16].into(),
        );
        let constant8: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([16, 16, 32], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [16, 16, 32].into(),
        );
        let conv2d1 = Conv2dConfig::new([1, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv2d2 = Conv2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 2])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(false)
            .init(device);
        let conv2d3 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv2d4 = Conv2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 2])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(false)
            .init(device);
        let conv2d5 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv2d6 = Conv2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(false)
            .init(device);
        let conv2d7 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv2d8 = Conv2dConfig::new([2, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(2)
            .with_bias(false)
            .init(device);
        let conv2d9 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv2d10 = Conv2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 2])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(false)
            .init(device);
        let conv2d11 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let gru1 = burn::nn::gru::GruConfig::new(256, 256, true)
            .with_reset_after(true)
            .init(device);
        let linear1 = LinearConfig::new(512, 1).with_bias(true).init(device);
        let constant79: burn::module::Param<Tensor<1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<1>::from_data(
                    burn::tensor::TensorData::from([50f64]),
                    (device, burn::tensor::DType::F32),
                )
            },
            device.clone(),
            false,
            [1].into(),
        );
        let constant80: burn::module::Param<Tensor<1>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<1>::from_data(
                    burn::tensor::TensorData::from([-15f64]),
                    (device, burn::tensor::DType::F32),
                )
            },
            device.clone(),
            false,
            [1].into(),
        );
        Self {
            constant6,
            constant7,
            constant8,
            conv2d1,
            conv2d2,
            conv2d3,
            conv2d4,
            conv2d5,
            conv2d6,
            conv2d7,
            conv2d8,
            conv2d9,
            conv2d10,
            conv2d11,
            gru1,
            linear1,
            constant79,
            constant80,
            device: device.clone(),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(
        &self,
        feat_erb: Tensor<4>,
        feat_spec: Tensor<4>,
    ) -> (
        Tensor<4>,
        Tensor<4>,
        Tensor<4>,
        Tensor<4>,
        Tensor<3>,
        Tensor<4>,
        Tensor<3>,
    ) {
        let constant6_out1 = self.constant6.val();
        let constant7_out1 = self.constant7.val();
        let constant8_out1 = self.constant8.val();
        let constant24_out1: [i64; 4] = [0i64, 0i64, 2i64, 0i64];
        let constantofshape1_out1: [i64; 4usize] = [0i64, 0i64, 0i64, 0i64];
        let concat1_out1: [i64; 8usize] = [&constant24_out1[..], &constantofshape1_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape1_out1 = {
            let shape_array = concat1_out1 as [i64; 8usize];
            Tensor::<1, Int>::from_data(
                TensorData::from(shape_array),
                (&self.device, burn::tensor::DType::I64),
            )
        }
        .reshape([-1, 2]);
        let slice1_out1 = {
            let slice_input = reshape1_out1;
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
        let transpose1_out1 = slice1_out1.permute([1, 0]);
        let reshape2_out1 = transpose1_out1.reshape([-1]);
        let pad1_out1 = feat_erb.pad(
            {
                let __raw: alloc::vec::Vec<i64> = reshape2_out1
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
        let relu1_out1 = burn::tensor::activation::relu(conv2d1_out1);
        let conv2d2_out1 = self.conv2d2.forward(relu1_out1.clone());
        let conv2d3_out1 = self.conv2d3.forward(conv2d2_out1);
        let relu2_out1 = burn::tensor::activation::relu(conv2d3_out1);
        let conv2d4_out1 = self.conv2d4.forward(relu2_out1.clone());
        let conv2d5_out1 = self.conv2d5.forward(conv2d4_out1);
        let relu3_out1 = burn::tensor::activation::relu(conv2d5_out1);
        let conv2d6_out1 = self.conv2d6.forward(relu3_out1.clone());
        let conv2d7_out1 = self.conv2d7.forward(conv2d6_out1);
        let relu4_out1 = burn::tensor::activation::relu(conv2d7_out1);
        let constant33_out1: [i64; 4] = [0i64, 0i64, 2i64, 0i64];
        let constantofshape2_out1: [i64; 4usize] = [0i64, 0i64, 0i64, 0i64];
        let concat2_out1: [i64; 8usize] = [&constant33_out1[..], &constantofshape2_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape3_out1 = {
            let shape_array = concat2_out1 as [i64; 8usize];
            Tensor::<1, Int>::from_data(
                TensorData::from(shape_array),
                (&self.device, burn::tensor::DType::I64),
            )
        }
        .reshape([-1, 2]);
        let slice2_out1 = {
            let slice_input = reshape3_out1;
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
        let transpose2_out1 = slice2_out1.permute([1, 0]);
        let reshape4_out1 = transpose2_out1.reshape([-1]);
        let pad2_out1 = feat_spec.pad(
            {
                let __raw: alloc::vec::Vec<i64> = reshape4_out1
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
        let conv2d8_out1 = self.conv2d8.forward(pad2_out1);
        let conv2d9_out1 = self.conv2d9.forward(conv2d8_out1);
        let relu5_out1 = burn::tensor::activation::relu(conv2d9_out1);
        let conv2d10_out1 = self.conv2d10.forward(relu5_out1.clone());
        let conv2d11_out1 = self.conv2d11.forward(conv2d10_out1);
        let relu6_out1 = burn::tensor::activation::relu(conv2d11_out1);
        let transpose3_out1 = relu6_out1.permute([0, 2, 3, 1]);
        let shape1_out1: [i64; 4] = {
            let axes = &transpose3_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice3_out1: [i64; 2] = shape1_out1[0..2].try_into().unwrap();
        let constant44_out1: [i64; 1] = [-1i64];
        let concat3_out1: [i64; 3usize] = [&slice3_out1[..], &constant44_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape5_out1 = transpose3_out1.reshape(concat3_out1);
        let shape2_out1: [i64; 3] = {
            let axes = &reshape5_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather1_out1 = shape2_out1[0] as i64;
        let gather2_out1 = shape2_out1[1] as i64;
        let unsqueeze1_out1 = [gather1_out1 as i64];
        let unsqueeze2_out1 = [gather2_out1 as i64];
        let constant47_out1: [i64; 1] = [32i64];
        let constant48_out1: [i64; 1] = [96i64];
        let concat4_out1: [i64; 4usize] = [
            &unsqueeze1_out1[..],
            &unsqueeze2_out1[..],
            &constant47_out1[..],
            &constant48_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape6_out1 = reshape5_out1.reshape(concat4_out1);
        let einsum1_out1 = {
            let einsum_lhs = reshape6_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant6_out1;
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
        let slice4_out1: [i64; 2] = shape3_out1[0..2].try_into().unwrap();
        let constant52_out1: [i64; 1] = [-1i64];
        let concat5_out1: [i64; 3usize] = [&slice4_out1[..], &constant52_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape7_out1 = einsum1_out1.reshape(concat5_out1);
        let relu7_out1 = burn::tensor::activation::relu(reshape7_out1);
        let transpose4_out1 = relu4_out1.clone().permute([0, 2, 3, 1]);
        let shape4_out1: [i64; 4] = {
            let axes = &transpose4_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice5_out1: [i64; 2] = shape4_out1[0..2].try_into().unwrap();
        let constant56_out1: [i64; 1] = [-1i64];
        let concat6_out1: [i64; 3usize] = [&slice5_out1[..], &constant56_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape8_out1 = transpose4_out1.reshape(concat6_out1);
        let add1_out1 = reshape8_out1.add(relu7_out1);
        let shape5_out1: [i64; 3] = {
            let axes = &add1_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather3_out1 = shape5_out1[0] as i64;
        let gather4_out1 = shape5_out1[1] as i64;
        let unsqueeze3_out1 = [gather3_out1 as i64];
        let unsqueeze4_out1 = [gather4_out1 as i64];
        let constant59_out1: [i64; 1] = [16i64];
        let constant60_out1: [i64; 1] = [32i64];
        let concat7_out1: [i64; 4usize] = [
            &unsqueeze3_out1[..],
            &unsqueeze4_out1[..],
            &constant59_out1[..],
            &constant60_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape9_out1 = add1_out1.reshape(concat7_out1);
        let einsum2_out1 = {
            let einsum_lhs = reshape9_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant7_out1;
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
        let shape6_out1: [i64; 4] = {
            let axes = &einsum2_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice6_out1: [i64; 2] = shape6_out1[0..2].try_into().unwrap();
        let constant64_out1: [i64; 1] = [-1i64];
        let concat8_out1: [i64; 3usize] = [&slice6_out1[..], &constant64_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape10_out1 = einsum2_out1.reshape(concat8_out1);
        let relu8_out1 = burn::tensor::activation::relu(reshape10_out1);
        let shape7_out1: [i64; 3] = {
            let axes = &relu8_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather5_out1 = shape7_out1[0] as i64;
        let constant66_out1: [i64; 1] = [1i64];
        let unsqueeze5_out1 = [gather5_out1 as i64];
        let constant67_out1: [i64; 1] = [256i64];
        let concat9_out1: [i64; 3usize] = [
            &constant66_out1[..],
            &unsqueeze5_out1[..],
            &constant67_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let constantofshape3_out1 = Tensor::<1>::from_data(
            burn::tensor::TensorData::from([0f32 as f64]),
            (&self.device, burn::tensor::DType::F32),
        )
        .reshape([1, 1, 1])
        .expand(concat9_out1);
        let transpose5_out1 = relu8_out1.permute([1, 0, 2]);
        let (gru1_out1, gru1_out2) = {
            let gru_output = self.gru1.forward(
                transpose5_out1.swap_dims(0, 1),
                Some(constantofshape3_out1.squeeze_dim(0)),
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
        let transpose6_out1 = squeeze1_out1.permute([1, 0, 2]);
        let shape8_out1: [i64; 3] = {
            let axes = &transpose6_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather6_out1 = shape8_out1[0] as i64;
        let gather7_out1 = shape8_out1[1] as i64;
        let unsqueeze6_out1 = [gather6_out1 as i64];
        let unsqueeze7_out1 = [gather7_out1 as i64];
        let constant73_out1: [i64; 1] = [16i64];
        let constant74_out1: [i64; 1] = [16i64];
        let concat10_out1: [i64; 4usize] = [
            &unsqueeze6_out1[..],
            &unsqueeze7_out1[..],
            &constant73_out1[..],
            &constant74_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape11_out1 = transpose6_out1.reshape(concat10_out1);
        let einsum3_out1 = {
            let einsum_lhs = reshape11_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant8_out1;
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
        let shape9_out1: [i64; 4] = {
            let axes = &einsum3_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice7_out1: [i64; 2] = shape9_out1[0..2].try_into().unwrap();
        let constant78_out1: [i64; 1] = [-1i64];
        let concat11_out1: [i64; 3usize] = [&slice7_out1[..], &constant78_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape12_out1 = einsum3_out1.reshape(concat11_out1);
        let relu9_out1 = burn::tensor::activation::relu(reshape12_out1);
        let linear1_out1 = self.linear1.forward(relu9_out1.clone());
        let sigmoid1_out1 = burn::tensor::activation::sigmoid(linear1_out1);
        let constant79_out1 = self.constant79.val();
        let mul1_out1 = sigmoid1_out1.mul((constant79_out1).unsqueeze_dims(&[0isize, 1isize]));
        let constant80_out1 = self.constant80.val();
        let add2_out1 = mul1_out1.add((constant80_out1).unsqueeze_dims(&[0isize, 1isize]));
        (
            relu1_out1, relu2_out1, relu3_out1, relu4_out1, relu9_out1, relu5_out1, add2_out1,
        )
    }
}
