use scroll_zkvm_types_chunk::Crypto;
use scroll_zkvm_types_circuit::{Circuit, public_inputs::PublicInputs, reveal_pi_hash};

mod circuit;
use circuit::ChunkCircuit as C;

openvm::entry!(main);

fn main() {
    Crypto::install();

    ecies::sha256::set_digest_provider(|| {
        Box::new(ecies::sha256::ext::ExtSha256Core::new(
            openvm_sha2::set_sha256,
        ))
    })
    .unwrap();

    let witness_bytes = C::read_witness_bytes();

    #[cfg(feature = "witness-memory-diagnostics")]
    openvm::io::println(format!(
        "witness-stage: input-read bytes={}",
        witness_bytes.len()
    ));

    // The OpenVM guest allocator never reclaims this buffer. Explicitly leaking
    // it gives the zero-copy deserializer a sound static lifetime; host execution
    // keeps the ordinary owning decoder and releases its input normally.
    #[cfg(target_os = "zkvm")]
    let witness_bytes: &'static [u8] = witness_bytes.leak();
    #[cfg(target_os = "zkvm")]
    let witness = C::deserialize_witness_from_static(witness_bytes);
    #[cfg(not(target_os = "zkvm"))]
    let witness = C::deserialize_witness(&witness_bytes);

    #[cfg(all(target_os = "zkvm", feature = "witness-memory-diagnostics"))]
    check_borrowed_witness(&witness, witness_bytes);
    #[cfg(feature = "witness-memory-diagnostics")]
    openvm::io::println("witness-stage: validation-start");

    let public_inputs = C::validate(witness);

    #[cfg(feature = "witness-memory-diagnostics")]
    openvm::io::println("witness-stage: validation-complete");

    reveal_pi_hash(public_inputs.pi_hash());
}

#[cfg(all(target_os = "zkvm", feature = "witness-memory-diagnostics"))]
fn check_borrowed_witness(witness: &<C as Circuit>::Witness, input: &[u8]) {
    let range = input.as_ptr_range();
    let mut code_bytes = 0;
    let mut state_bytes = 0;
    let mut borrowed_strings = 0;
    let mut unaligned_code_count = 0;
    let mut unaligned_code_bytes = 0;
    let mut unaligned_state_count = 0;
    let mut unaligned_state_bytes = 0;
    for block in &witness.witness().blocks {
        code_bytes += block.codes.iter().map(|code| code.len()).sum::<usize>();
        state_bytes += block.states.iter().map(|state| state.len()).sum::<usize>();
        for code in block.codes.iter().filter(|code| !code.is_empty()) {
            if !(code.as_ptr() as usize).is_multiple_of(4) {
                unaligned_code_count += 1;
                unaligned_code_bytes += code.len();
            }
        }
        for state in block.states.iter().filter(|state| !state.is_empty()) {
            if !(state.as_ptr() as usize).is_multiple_of(4) {
                unaligned_state_count += 1;
                unaligned_state_bytes += state.len();
            }
        }
        for bytes in block.codes.iter().chain(&block.states) {
            if bytes.is_empty() {
                continue;
            }
            let bytes_range = bytes.as_ptr_range();
            assert!(bytes_range.start >= range.start && bytes_range.end <= range.end);
            let cloned = bytes.clone();
            assert_eq!(cloned.as_ptr(), bytes.as_ptr());
            borrowed_strings += 1;
        }
    }
    openvm::io::println(format!(
        "witness-stage: deserialized code_bytes={code_bytes} state_bytes={state_bytes} borrowed_strings={borrowed_strings}"
    ));
    openvm::io::println(format!(
        "witness-alignment: unaligned_code_count={unaligned_code_count} unaligned_code_bytes={unaligned_code_bytes} unaligned_state_count={unaligned_state_count} unaligned_state_bytes={unaligned_state_bytes}"
    ));
}
