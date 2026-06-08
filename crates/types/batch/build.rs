use alloy_primitives::{U256, hex};
use eyre::ContextCompat;
use halo2curves_axiom::bls12_381::Fr;
use halo2curves_axiom::bls12_381::G2Affine;
use halo2curves_axiom::ff::{Field, PrimeField};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const BLOB_WIDTH: usize = 4096;
const LOG_BLOB_WIDTH: usize = 12;

fn main() -> eyre::Result<()> {
    let out_dir = std::env::var("OUT_DIR")?;
    let out_dir = Path::new(&out_dir).join("kzg");
    fs::create_dir_all(&out_dir)?;

    gen_roots_of_unity(&out_dir)?;
    write_g2_generator(&out_dir)?;
    write_kzg_g2_setup(&out_dir)?;

    Ok(())
}

fn gen_roots_of_unity(out_dir: &Path) -> eyre::Result<()> {
    let out_file_path = Path::new(&out_dir).join("roots_of_unity.rs");
    let mut f = File::create(out_file_path)?;

    // https://github.com/ethereum/consensus-specs/blob/master/specs/deneb/polynomial-commitments.md#constants
    let primitive_root_of_unity = Fr::from(7u64);
    let modulus = U256::from_str_radix(&Fr::MODULUS[2..], 16)?;

    let exponent = (modulus - U256::from(1)) / U256::from(4096);
    let root_of_unity = primitive_root_of_unity.pow(exponent.as_limbs());

    let mut ascending_order: Vec<Fr> = Vec::new();
    ascending_order.resize(BLOB_WIDTH, Fr::ZERO);
    ascending_order[0] = Fr::ONE; // First element should be 1

    for i in 1..BLOB_WIDTH {
        let (left, right) = ascending_order.split_at_mut(i);
        right[0] += &left[left.len() - 1];
        right[0] *= &root_of_unity;
    }

    let roots_of_unity = (0..BLOB_WIDTH).map(|i| {
        let j = u16::try_from(i).unwrap().reverse_bits() >> (16 - LOG_BLOB_WIDTH);
        ascending_order[usize::from(j)]
    });

    writeln!(
        f,
        "static ROOTS_OF_UNITY: [::openvm_pairing::bls12_381::Scalar; {}] = [",
        BLOB_WIDTH
    )?;
    for root_of_unity in roots_of_unity {
        writeln!(
            f,
            "    ::openvm_pairing::bls12_381::Scalar::from_const_bytes("
        )?;
        writeln!(
            f,
            "        ::alloy_primitives::hex!(\"{}\"),",
            hex::encode(root_of_unity.to_bytes())
        )?;
        writeln!(f, "    ),")?;
    }
    writeln!(f, "];")?;

    Ok(())
}

fn write_g2_generator(out_dir: &Path) -> eyre::Result<()> {
    let out_file_path = Path::new(&out_dir).join("g2_generator.rs");
    let mut f = File::create(out_file_path)?;
    write_g2(&mut f, "G2_GENERATOR", &G2Affine::generator())?;
    Ok(())
}

fn write_kzg_g2_setup(out_dir: &Path) -> eyre::Result<()> {
    let out_file_path = Path::new(&out_dir).join("kzg_g2_setup.rs");
    let mut f = File::create(out_file_path)?;

    // Use the second G2 field in kzg setup (G2[1]),
    // extracted from https://github.com/ethereum/c-kzg-4844/blob/81a8949f29d27d225ca74ebb4e9061bdd100560a/src/trusted_setup.txt#L4100
    const KZG_G2_SETUP_BYTES_COMPRESSED: [u8; 96] = hex!(
        "b5bfd7dd8cdeb128843bc287230af38926187075cbfbefa81009a2ce615ac53d2914e5870cb452d2afaaab24f3499f72185cbfee53492714734429b7b38608e23926c911cceceac9a36851477ba4c60b087041de621000edc98edada20c1def2"
    );
    let p = G2Affine::from_compressed_be(&KZG_G2_SETUP_BYTES_COMPRESSED)
        .into_option()
        .context("invalid KZG G2 setup compressed bytes")?;

    write_g2(&mut f, "KZG_G2_SETUP", &p)?;
    Ok(())
}

fn write_g2(f: &mut File, name: &str, p: &G2Affine) -> eyre::Result<()> {
    writeln!(
        f,
        "static {name}: ::openvm_pairing::bls12_381::G2Affine = ::openvm_pairing::bls12_381::G2Affine::new("
    )?;
    writeln!(f, "    ::openvm_pairing::bls12_381::Fp2::new(")?;
    writeln!(
        f,
        "        ::openvm_pairing::bls12_381::Fp::from_const_bytes("
    )?;
    writeln!(
        f,
        "            ::alloy_primitives::hex!(\"{}\"),",
        hex::encode(p.x.c0.to_bytes())
    )?;
    writeln!(f, "        ),")?;
    writeln!(
        f,
        "        ::openvm_pairing::bls12_381::Fp::from_const_bytes("
    )?;
    writeln!(
        f,
        "            ::alloy_primitives::hex!(\"{}\"),",
        hex::encode(p.x.c1.to_bytes())
    )?;
    writeln!(f, "        ),")?;
    writeln!(f, "    ),")?;
    writeln!(f, "    ::openvm_pairing::bls12_381::Fp2::new(")?;
    writeln!(
        f,
        "        ::openvm_pairing::bls12_381::Fp::from_const_bytes("
    )?;
    writeln!(
        f,
        "            ::alloy_primitives::hex!(\"{}\"),",
        hex::encode(p.y.c0.to_bytes())
    )?;
    writeln!(f, "        ),")?;
    writeln!(
        f,
        "        ::openvm_pairing::bls12_381::Fp::from_const_bytes("
    )?;
    writeln!(
        f,
        "            ::alloy_primitives::hex!(\"{}\"),",
        hex::encode(p.y.c1.to_bytes())
    )?;
    writeln!(f, "        ),")?;
    writeln!(f, "    ),")?;
    writeln!(f, ");")?;

    Ok(())
}
