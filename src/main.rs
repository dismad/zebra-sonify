use clap::Parser;
use flate2::{write::ZlibEncoder, Compression};
use hound::{WavSpec, WavWriter};
use reqwest::blocking::Client;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::f32::consts::PI;

// ==================== CLI ====================
#[derive(Parser, Debug)]
#[command(author, version, about = "Sonify Zcash txs from zebrad into WAV")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8232")]
    node: String,

    #[arg(long)]
    cookie_file: Option<PathBuf>,

    #[arg(long)]
    txid: Option<String>,

    #[arg(long)]
    txids_file: Option<String>,

    #[arg(long, default_value = "tx_sonification.wav")]
    output: String,
}

// ==================== Data Structures ====================
#[derive(Debug, Clone)]
pub struct CompactTx {
    pub txid: String,
    pub packed: String,
    pub packed_len: usize,
    pub original_json_len: usize,
}

#[derive(Debug, Clone)]
pub struct TxNote {
    pub txid: String,
    pub pitch: u8,
    pub start_time_ms: u64,
    pub duration_ms: u32,
    pub velocity: u8,
}

// ==================== Zebra Cookie Support ====================
fn get_zebra_credentials(cookie_path: Option<PathBuf>) -> anyhow::Result<(String, String)> {
    let path = cookie_path.unwrap_or_else(|| PathBuf::from("/var/lib/zebrad-rpc/.cookie"));

    if !path.exists() {
        anyhow::bail!("Zebra cookie file not found at {:?}", path);
    }

    let content = std::fs::read_to_string(&path)?;
    let parts: Vec<&str> = content.trim().splitn(2, ':').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid cookie file format");
    }

    Ok(("__cookie__".to_string(), parts[1].to_string()))
}

// ==================== Real tx_to_compact (zlib + base85) ====================
fn b85_encode(data: &[u8]) -> String {
    // Simple base85 encoder (RFC 1924 style)
    let mut result = String::new();
    let alphabet = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";

    let mut chunks = data.chunks_exact(4);
    for chunk in chunks.by_ref() {
        let mut num = u32::from_be_bytes(chunk.try_into().unwrap());
        let mut chars = [0u8; 5];
        for i in (0..5).rev() {
            chars[i] = alphabet.as_bytes()[(num % 85) as usize];
            num /= 85;
        }
        result.push_str(std::str::from_utf8(&chars).unwrap());
    }

    // Handle remaining bytes
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut buf = [0u8; 4];
        buf[..rem.len()].copy_from_slice(rem);
        let mut num = u32::from_be_bytes(buf);
        let mut chars = [0u8; 5];
        for i in (0..5).rev() {
            chars[i] = alphabet.as_bytes()[(num % 85) as usize];
            num /= 85;
        }
        let valid = 1 + rem.len(); // 2,3,4 or 5 chars
        result.push_str(std::str::from_utf8(&chars[..valid]).unwrap());
    }

    result
}

fn tx_to_compact(txid: &str, json: &Value) -> CompactTx {
    let canonical = json.to_string();
    let original_len = canonical.len();

    // zlib level 9 compression
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(canonical.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();

    // base85 encoding
    let packed = b85_encode(&compressed);
    let packed_len = packed.len();           // ← get length BEFORE moving

    CompactTx {
        txid: txid.to_string(),
        packed,                              // now safe to move
        packed_len,
        original_json_len: original_len,
    }
}

// ==================== RPC Fetch ====================
fn fetch_tx_json(node: &str, txid: &str, cookie_path: Option<PathBuf>) -> anyhow::Result<Value> {
    let (user, pass) = get_zebra_credentials(cookie_path)?;

    let client = Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "getrawtransaction",
        "params": [txid, 1],
        "id": 1
    });

    let resp: Value = client
        .post(node)
        .basic_auth(&user, Some(&pass))
        .json(&body)
        .send()?
        .json()?;

    if let Some(err) = resp.get("error") {
        anyhow::bail!("zebrad error: {}", err);
    }

    Ok(resp["result"].clone())
}

// ==================== Sonification ====================
impl From<&CompactTx> for TxNote {
    fn from(ctx: &CompactTx) -> Self {
        let bytes = ctx.packed.as_bytes();
        let len = ctx.packed_len as u32;

        let pitch = 48 + (bytes.first().unwrap_or(&0) % 36) + ((len % 18) as u8);
        let duration_ms = (len.clamp(120, 900) as u32) * 3;
        let velocity = 55 + (bytes.get(3).unwrap_or(&0) % 55);

        TxNote {
            txid: ctx.txid.clone(),
            pitch: pitch.clamp(40, 90),
            start_time_ms: 0,
            duration_ms,
            velocity: velocity.clamp(50, 115),
        }
    }
}

fn midi_to_freq(midi: u8) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

fn render_notes_to_wav(notes: &[TxNote], output_path: &str, sample_rate: u32) {
    let total_ms: u64 = notes.iter()
        .map(|n| n.start_time_ms + n.duration_ms as u64)
        .max()
        .unwrap_or(8000);

    let total_samples = ((total_ms as f64 / 1000.0) * sample_rate as f64) as usize + sample_rate as usize;
    let mut buffer = vec![0.0f32; total_samples];

    for note in notes {
        let freq = midi_to_freq(note.pitch);
        let start = ((note.start_time_ms as f64 / 1000.0) * sample_rate as f64) as usize;
        let dur = ((note.duration_ms as f64 / 1000.0) * sample_rate as f64) as usize;
        let amp = note.velocity as f32 / 127.0 * 0.65;

        for i in 0..dur {
            let idx = start + i;
            if idx >= buffer.len() { break; }
            let t = i as f32 / sample_rate as f32;
            let env = (1.0 - (i as f32 / dur as f32).powf(0.6)).min(1.0);
            buffer[idx] += (t * freq * 2.0 * PI).sin() * amp * env;
        }
    }

    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(output_path, spec).unwrap();
    for &s in &buffer {
        writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();

    println!("Wrote {} ({} notes)", output_path, notes.len());
}

// ==================== Main ====================
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut txids = Vec::new();

    if let Some(txid) = args.txid {
        txids.push(txid);
    }

    if let Some(path) = args.txids_file {
        let file = File::open(path)?;
        for line in BufReader::new(file).lines().flatten() {
            let id = line.trim();
            if !id.is_empty() {
                txids.push(id.to_string());
            }
        }
    }

    if txids.is_empty() {
        anyhow::bail!("Provide --txid or --txids-file");
    }

    println!("Fetching {} tx(s) from {}", txids.len(), args.node);

    let mut compact_txs = Vec::new();

    for txid in &txids {
        match fetch_tx_json(&args.node, txid, args.cookie_file.clone()) {
            Ok(json) => {
                let compact = tx_to_compact(txid, &json);
                println!("  {} → packed_len={} chars", txid, compact.packed_len);
                compact_txs.push(compact);
            }
            Err(e) => eprintln!("Failed to fetch {}: {}", txid, e),
        }
    }

    if compact_txs.is_empty() {
        anyhow::bail!("No transactions processed");
    }

    // Create notes with timing
    let mut notes: Vec<TxNote> = compact_txs.iter().map(TxNote::from).collect();
    let mut time = 0u64;
    for note in &mut notes {
        note.start_time_ms = time;
        time += (note.duration_ms as u64 * 3) / 4;
    }

    render_notes_to_wav(&notes, &args.output, 44100);
    Ok(())
}