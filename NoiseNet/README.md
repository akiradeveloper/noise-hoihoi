# NoiseNet

NoiseNet is reserved as an independent peer project for the Burn implementation
of the noise-reduction model. It is intentionally not a Cargo workspace member
through v0.2, whose only processor is bit-exact pass-through.

The first NoiseNet crate will implement the `AudioProcessor` integration used
by `noise-hoihoi-engine` in v0.3 without taking ownership of audio devices, the
GUI, or the virtual driver.
