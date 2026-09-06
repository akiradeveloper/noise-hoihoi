// Generated from the official DeepFilterNet3 erb_dec.onnx graph by Burn 0.22.0-pre.3.
extern crate alloc;
use burn::nn::PaddingConfig2d;
use burn::nn::conv::Conv2d;
use burn::nn::conv::Conv2dConfig;
use burn::nn::conv::ConvTranspose2d;
use burn::nn::conv::ConvTranspose2dConfig;
use burn::prelude::*;
use burn::tensor::Bytes;
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;

#[derive(Module, Debug)]
pub struct Model {
    constant1: burn::module::Param<Tensor<3>>,
    constant2: burn::module::Param<Tensor<3>>,
    gru1: burn::nn::gru::Gru,
    gru2: burn::nn::gru::Gru,
    conv2d1: Conv2d,
    conv2d2: Conv2d,
    conv2d3: Conv2d,
    conv2d4: Conv2d,
    convtranspose2d1: ConvTranspose2d,
    conv2d5: Conv2d,
    conv2d6: Conv2d,
    convtranspose2d2: ConvTranspose2d,
    conv2d7: Conv2d,
    conv2d8: Conv2d,
    conv2d9: Conv2d,
    #[module(skip)]
    device: Device,
}

impl Model {
    /// One streaming ERB decoder step with explicit GRU state.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn forward_stream(
        &self,
        embedding: Tensor<3>,
        e3: Tensor<4>,
        e2: Tensor<4>,
        e1: Tensor<4>,
        e0: Tensor<4>,
        state0: Tensor<2>,
        state1: Tensor<2>,
    ) -> (Tensor<4>, Tensor<2>, Tensor<2>) {
        use crate::network::grouped_linear;

        let input =
            burn::tensor::activation::relu(grouped_linear(embedding, self.constant1.val(), 16));
        let hidden0 = self.gru1.forward(input, Some(state0));
        let state0 = hidden0.clone().squeeze_dim(1);
        let hidden1 = self.gru2.forward(hidden0, Some(state1));
        let state1 = hidden1.clone().squeeze_dim(1);
        let decoded =
            burn::tensor::activation::relu(grouped_linear(hidden1, self.constant2.val(), 16))
                .reshape([1, 1, 8, 64])
                .permute([0, 3, 1, 2]);

        let decoded3 = burn::tensor::activation::relu(self.conv2d1.forward(e3)).add(decoded);
        let decoded3 =
            burn::tensor::activation::relu(self.conv2d3.forward(self.conv2d2.forward(decoded3)));
        let decoded2 = burn::tensor::activation::relu(self.conv2d4.forward(e2)).add(decoded3);
        let decoded2 = burn::tensor::activation::relu(
            self.conv2d5
                .forward(self.convtranspose2d1.forward(decoded2)),
        );
        let decoded1 = burn::tensor::activation::relu(self.conv2d6.forward(e1)).add(decoded2);
        let decoded1 = burn::tensor::activation::relu(
            self.conv2d7
                .forward(self.convtranspose2d2.forward(decoded1)),
        );
        let decoded0 = burn::tensor::activation::relu(self.conv2d8.forward(e0)).add(decoded1);
        let mask = burn::tensor::activation::sigmoid(self.conv2d9.forward(decoded0));
        (mask, state0, state1)
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
        let constant1: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([16, 32, 16], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [16, 32, 16].into(),
        );
        let constant2: burn::module::Param<Tensor<3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<3>::zeros([16, 16, 32], (device, burn::tensor::DType::F32))
            },
            device.clone(),
            false,
            [16, 16, 32].into(),
        );
        let gru1 = burn::nn::gru::GruConfig::new(256, 256, true)
            .with_reset_after(true)
            .init(device);
        let gru2 = burn::nn::gru::GruConfig::new(256, 256, true)
            .with_reset_after(true)
            .init(device);
        let conv2d1 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(true)
            .init(device);
        let conv2d2 = Conv2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 1])
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
        let conv2d4 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(true)
            .init(device);
        let convtranspose2d1 = ConvTranspose2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 2])
            .with_padding([0, 1])
            .with_padding_out([0, 1])
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
        let conv2d6 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(true)
            .init(device);
        let convtranspose2d2 = ConvTranspose2dConfig::new([64, 64], [1, 3])
            .with_stride([1, 2])
            .with_padding([0, 1])
            .with_padding_out([0, 1])
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
        let conv2d8 = Conv2dConfig::new([64, 64], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(64)
            .with_bias(true)
            .init(device);
        let conv2d9 = Conv2dConfig::new([64, 1], [1, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(0, 1, 0, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        Self {
            constant1,
            constant2,
            gru1,
            gru2,
            conv2d1,
            conv2d2,
            conv2d3,
            conv2d4,
            convtranspose2d1,
            conv2d5,
            conv2d6,
            convtranspose2d2,
            conv2d7,
            conv2d8,
            conv2d9,
            device: device.clone(),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(
        &self,
        emb: Tensor<3>,
        e3: Tensor<4>,
        e2: Tensor<4>,
        e1: Tensor<4>,
        e0: Tensor<4>,
    ) -> Tensor<4> {
        let constant1_out1 = self.constant1.val();
        let constant2_out1 = self.constant2.val();
        let shape1_out1: [i64; 4] = {
            let axes = &e3.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather1_out1 = shape1_out1[0] as i64;
        let gather2_out1 = shape1_out1[2] as i64;
        let gather3_out1 = shape1_out1[3] as i64;
        let shape2_out1: [i64; 3] = {
            let axes = &emb.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather4_out1 = shape2_out1[0] as i64;
        let gather5_out1 = shape2_out1[1] as i64;
        let unsqueeze1_out1 = [gather4_out1 as i64];
        let unsqueeze2_out1 = [gather5_out1 as i64];
        let constant27_out1: [i64; 1] = [16i64];
        let constant28_out1: [i64; 1] = [32i64];
        let concat1_out1: [i64; 4usize] = [
            &unsqueeze1_out1[..],
            &unsqueeze2_out1[..],
            &constant27_out1[..],
            &constant28_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape1_out1 = emb.reshape(concat1_out1);
        let einsum1_out1 = {
            let einsum_lhs = reshape1_out1.permute([2usize, 0usize, 1usize, 3usize]);
            let einsum_rhs = constant1_out1;
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
        let constant32_out1: [i64; 1] = [-1i64];
        let concat2_out1: [i64; 3usize] = [&slice1_out1[..], &constant32_out1[..]]
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
        let gather6_out1 = shape4_out1[0] as i64;
        let constant34_out1: [i64; 1] = [2i64];
        let unsqueeze3_out1 = [gather6_out1 as i64];
        let constant35_out1: [i64; 1] = [256i64];
        let concat3_out1: [i64; 3usize] = [
            &constant34_out1[..],
            &unsqueeze3_out1[..],
            &constant35_out1[..],
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
        let shape5_out1: [i64; 3] = {
            let axes = &transpose2_out1.clone().dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let gather7_out1 = shape5_out1[0] as i64;
        let gather8_out1 = shape5_out1[1] as i64;
        let unsqueeze4_out1 = [gather7_out1 as i64];
        let unsqueeze5_out1 = [gather8_out1 as i64];
        let constant50_out1: [i64; 1] = [16i64];
        let constant51_out1: [i64; 1] = [16i64];
        let concat4_out1: [i64; 4usize] = [
            &unsqueeze4_out1[..],
            &unsqueeze5_out1[..],
            &constant50_out1[..],
            &constant51_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape3_out1 = transpose2_out1.reshape(concat4_out1);
        let einsum2_out1 = {
            let einsum_lhs = reshape3_out1.permute([2usize, 0usize, 1usize, 3usize]);
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
        let shape6_out1: [i64; 4] = {
            let axes = &einsum2_out1.clone().dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let slice4_out1: [i64; 2] = shape6_out1[0..2].try_into().unwrap();
        let constant55_out1: [i64; 1] = [-1i64];
        let concat5_out1: [i64; 3usize] = [&slice4_out1[..], &constant55_out1[..]]
            .concat()
            .try_into()
            .unwrap();
        let reshape4_out1 = einsum2_out1.reshape(concat5_out1);
        let relu2_out1 = burn::tensor::activation::relu(reshape4_out1);
        let unsqueeze6_out1 = [gather1_out1 as i64];
        let unsqueeze7_out1 = [gather2_out1 as i64];
        let unsqueeze8_out1 = [gather3_out1 as i64];
        let constant56_out1: [i64; 1] = [-1i64];
        let concat6_out1: [i64; 4usize] = [
            &unsqueeze6_out1[..],
            &unsqueeze7_out1[..],
            &unsqueeze8_out1[..],
            &constant56_out1[..],
        ]
        .concat()
        .try_into()
        .unwrap();
        let reshape5_out1 = relu2_out1.reshape(concat6_out1);
        let transpose3_out1 = reshape5_out1.permute([0, 3, 1, 2]);
        let conv2d1_out1 = self.conv2d1.forward(e3);
        let relu3_out1 = burn::tensor::activation::relu(conv2d1_out1);
        let add1_out1 = relu3_out1.add(transpose3_out1);
        let conv2d2_out1 = self.conv2d2.forward(add1_out1);
        let conv2d3_out1 = self.conv2d3.forward(conv2d2_out1);
        let relu4_out1 = burn::tensor::activation::relu(conv2d3_out1);
        let conv2d4_out1 = self.conv2d4.forward(e2);
        let relu5_out1 = burn::tensor::activation::relu(conv2d4_out1);
        let add2_out1 = relu5_out1.add(relu4_out1);
        let convtranspose2d1_out1 = self.convtranspose2d1.forward(add2_out1);
        let conv2d5_out1 = self.conv2d5.forward(convtranspose2d1_out1);
        let relu6_out1 = burn::tensor::activation::relu(conv2d5_out1);
        let conv2d6_out1 = self.conv2d6.forward(e1);
        let relu7_out1 = burn::tensor::activation::relu(conv2d6_out1);
        let add3_out1 = relu7_out1.add(relu6_out1);
        let convtranspose2d2_out1 = self.convtranspose2d2.forward(add3_out1);
        let conv2d7_out1 = self.conv2d7.forward(convtranspose2d2_out1);
        let relu8_out1 = burn::tensor::activation::relu(conv2d7_out1);
        let conv2d8_out1 = self.conv2d8.forward(e0);
        let relu9_out1 = burn::tensor::activation::relu(conv2d8_out1);
        let add4_out1 = relu9_out1.add(relu8_out1);
        let conv2d9_out1 = self.conv2d9.forward(add4_out1);
        let sigmoid1_out1 = burn::tensor::activation::sigmoid(conv2d9_out1);
        sigmoid1_out1
    }
}
