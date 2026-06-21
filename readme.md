# Zcash Transaction Sonification Pipeline

## Overview

This project transforms verbose JSON data from a Zcash node (`zebrad`) into a compact, reversible representation and then into music.

The goal is to explore creative and practical ways to handle blockchain data — making large transaction responses smaller while also turning them into something audible.

## The Core Problem

`getrawtransaction` responses from `zebrad` (and most blockchain nodes) are very verbose. A single transaction can easily be 1.5k–3k+ characters when returned as pretty JSON. This creates challenges for:

- Storage
- Transmission
- Embedding in UIs or small messages
- Creative applications

## The Solution: A Complete Pipeline

### 1. Fetch Real Data
- Connects to a running `zebrad` node via JSON-RPC
- Uses cookie-based authentication (the default in modern Zebra)
- Supports both single txids and lists of txids from a file

### 2. Compress into a Compact Reversible String
Each transaction JSON goes through this transformation:

1. **Minify** the JSON (remove unnecessary whitespace)
2. **Compress** using `zlib` (level 9)
3. **Encode** the binary result using **Base85**

This produces a single printable ASCII string that is:
- Much smaller than the original JSON
- Fully reversible (you can decode it back to the original data)
- Safe to store or transmit as text

The result is stored in a `CompactTx` struct along with useful metadata:
- Original JSON length
- Packed string length
- The compact string itself

### 3. Sonification (Data → Music)
Each `CompactTx` is deterministically mapped into a musical note:

- `packed_len` and bytes from the packed string influence **pitch**
- Length values influence **duration**
- Byte values influence **velocity** (loudness)

Because the input (txid + packed data) is unique, the resulting musical event is also unique but fully reproducible.

Multiple transactions are sequenced over time to create a short musical piece.

### 4. Audio Rendering
The sequence of notes is rendered into a real `.wav` file using:
- Simple sine wave synthesis
- Basic amplitude envelope (to avoid clicks)
- 44.1kHz mono audio

The output is a standard WAV file you can play in any audio player or import into a DAW.

## Key Concepts

### Reversible Compression + Text Encoding
We don’t just compress — we create a **lossless, round-trippable** representation. The packed string contains everything needed to reconstruct the original JSON (or a slimmed version of it).

### Deterministic Sonification
The mapping from data → music is deterministic. The same transaction will always produce the same note characteristics. This makes the system reproducible and debuggable.

### Metadata Matters
Keeping track of both the original size and the packed size is useful for:
- Understanding compression effectiveness
- Deciding whether something fits size constraints (e.g. 511 characters)
- Building indexes or dashboards

### Creative Use of Blockchain Data
Instead of treating transactions only as financial or technical data, we treat them as a source of unique, structured information that can drive generative art — in this case, music.

## Architecture

```
zebrad RPC
    ↓
Raw Transaction JSON
    ↓
Minify → Zlib Compress → Base85 Encode
    ↓
CompactTx { txid, packed, packed_len, original_len }
    ↓
Deterministic Mapping
    ↓
TxNote { pitch, duration, velocity, timing }
    ↓
Audio Synthesis → WAV File
```

## Why This Approach?

| Goal                    | How We Achieve It                          |
|-------------------------|--------------------------------------------|
| Small size              | zlib + Base85 encoding                     |
| Reversibility           | Lossless compression + deterministic encoding |
| Uniqueness per tx       | Data-driven mapping from packed content    |
| Playable output         | Standard WAV rendering                     |
| Real node integration   | Cookie-authenticated JSON-RPC              |
| Extensibility           | Clean `CompactTx` / `TxNote` abstractions  |

## Converting to mp4

```bash
ffmpeg -loop 1 -i waveform.png -i input.wav \
  -c:v libx264 -preset medium -crf 23 \
  -c:a aac -b:a 128k -shortest \
  -pix_fmt yuv420p -movflags +faststart \
  output.mp4
```

## Future Possibilities

- Better synthesis (multiple oscillators, filters, real instrument samples)
- Chord generation per transaction
- Visualizer (egui app showing both waveform and blockchain data)
- Batch processing of entire blocks
- Different musical styles per transaction type (transparent vs shielded)
- Integration with real-time playback while syncing

## Summary

This system demonstrates a full pipeline from **raw blockchain RPC data → compact reversible representation → creative sonic output**.

It combines practical engineering (efficient compression, proper node authentication) with creative exploration (data sonification), showing that blockchain data can be both useful *and* expressive.
