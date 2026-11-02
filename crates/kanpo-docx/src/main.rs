//! `kanpo-docx` — 官報の PDF を DOCX にする CLI。
//!
//! 既定で、ページの画像（スキャンした本文）を ndlocr-lite-rs で OCR する（`ndlocr-lite-rs recognize-page
//! --trace-json` を外部コマンドとして呼ぶ）。`--no-ocr` なら PDF の字だけで書く。`--ocr-dir` を渡すと、
//! 認識し直さずに置いてある trace JSON を読む。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Parser;
use kanpo_docx::{OcrEngine, OcrPage, OcrRegion, PageImage};

#[derive(Parser)]
#[command(name = "kanpo-docx", about = "官報の PDF を DOCX にする（画像の本文は ndlocr-lite-rs で OCR する）")]
struct Cli {
    /// 官報の PDF。
    input: PathBuf,
    /// 書き出す DOCX（省略時は入力の拡張子を .docx にしたもの）。
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// OCR しない（PDF の字だけで書く）。
    #[arg(long)]
    no_ocr: bool,
    /// 認識し直さずに読む OCR の結果（NDLOCR の trace JSON）の置き場所。1 ページの PDF は `{dir}/{PDF の名前}.json`、
    /// 複数ページの PDF は `{dir}/{PDF の名前}-{1 始まりのページ}.json`。無いページは OCR しない。
    #[arg(long, conflicts_with = "no_ocr")]
    ocr_dir: Option<PathBuf>,
    /// ndlocr-lite-rs の実行ファイル（`--features onnx` でビルドしたもの）。
    #[arg(long, env = "NDLOCR_BIN", default_value = "ndlocr-lite-rs")]
    ndlocr: PathBuf,
    /// ndlocr-lite-rs のモデルの置き場所（`deim-s-1024x1024.onnx`、parseq の 3 つ、`NDLmoji.yaml`）。
    #[arg(long, env = "NDLOCR_MODELS", default_value = "models")]
    models: PathBuf,
    /// 取り出したページの画像を PNG で書き出す（確認用）。
    #[arg(long)]
    dump_images: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let pdf = std::fs::read(&cli.input).with_context(|| format!("read {}", cli.input.display()))?;
    let output = cli.output.clone().unwrap_or_else(|| cli.input.with_extension("docx"));
    if let Some(dir) = &cli.dump_images {
        std::fs::create_dir_all(dir)?;
        for (k, img) in kanpo_docx::page_images(&pdf)?.iter().enumerate() {
            let path = dir.join(format!("p{:04}-{k}.png", img.page + 1));
            write_png(&path, img)?;
            eprintln!("wrote {}", path.display());
        }
    }
    let ocr: Vec<Option<OcrPage>> = if cli.no_ocr {
        Vec::new()
    } else if let Some(dir) = &cli.ocr_dir {
        traces_from_dir(&cli.input, &pdf, dir)?
    } else {
        let mut engine = NdlocrCommand { bin: cli.ndlocr.clone(), models: cli.models.clone(), tmp: tempdir()? };
        kanpo_docx::recognize_pages(&pdf, &mut engine)?
    };
    let docx = kanpo_docx::convert(&pdf, &ocr)?;
    std::fs::write(&output, docx).with_context(|| format!("write {}", output.display()))?;
    let pages = ocr.iter().filter(|p| p.is_some()).count();
    eprintln!("wrote {} ({pages} page(s) with OCR)", output.display());
    Ok(())
}

fn traces_from_dir(input: &Path, pdf: &[u8], dir: &Path) -> Result<Vec<Option<OcrPage>>> {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let n = kanpo_docx::page_count(pdf)?;
    (0..n)
        .map(|i| {
            let name = if n == 1 { format!("{stem}.json") } else { format!("{stem}-{}.json", i + 1) };
            match std::fs::read_to_string(dir.join(&name)) {
                Ok(json) => OcrPage::from_ndlocr_trace(&json).map(Some).with_context(|| name.clone()),
                Err(_) => Ok(None),
            }
        })
        .collect()
}

/// ndlocr-lite-rs の `recognize-page` を外部コマンドとして呼ぶ OCR。
struct NdlocrCommand {
    bin: PathBuf,
    models: PathBuf,
    tmp: PathBuf,
}

impl OcrEngine for NdlocrCommand {
    fn recognize(&mut self, image: &PageImage) -> Result<Vec<OcrRegion>> {
        let png = self.tmp.join(format!("p{:04}.png", image.page + 1));
        let trace = self.tmp.join(format!("p{:04}.json", image.page + 1));
        write_png(&png, image)?;
        let m = |name: &str| self.models.join(name);
        let status = Command::new(&self.bin)
            .arg("recognize-page")
            .arg("--image")
            .arg(&png)
            .arg("--trace-json")
            .arg(&trace)
            .arg("--output-txt")
            .arg(self.tmp.join("out.txt"))
            .args(["--structure-rules", "false"])
            .arg("--det-model")
            .arg(m("deim-s-1024x1024.onnx"))
            .arg("--model")
            .arg(m("parseq-ndl-24x768-100-tiny-153epoch-tegaki3-r8data-202604.onnx"))
            .arg("--model30")
            .arg(m("parseq-ndl-24x256-30-tiny-189epoch-tegaki3-r8data-202604.onnx"))
            .arg("--model50")
            .arg(m("parseq-ndl-24x384-50-tiny-300epoch-tegaki3-r8data-202604.onnx"))
            .arg("--charset")
            .arg(m("NDLmoji.yaml"))
            .status()
            .with_context(|| format!("run {} (set --ndlocr or NDLOCR_BIN)", self.bin.display()))?;
        if !status.success() {
            bail!("{} recognize-page failed on page {}: {status}", self.bin.display(), image.page + 1);
        }
        let page = OcrPage::from_ndlocr_trace(&std::fs::read_to_string(&trace)?)?;
        Ok(page.regions)
    }
}

fn tempdir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("kanpo-docx-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write_png(path: &Path, image: &PageImage) -> Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, image.image.width, image.image.height);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&image.image.pixels)?;
    Ok(())
}
