# PNG Encoding (`src/plot/png.rs`)

The `png` module implements a minimal, dependency-free PNG encoder. It allows Aurora to export charts as valid PNG files without requiring a heavy compression library.

## Implementation Strategy: "Stored" Blocks

The PNG specification uses the DEFLATE compression algorithm. To avoid including a full zlib/deflate implementation, Aurora uses **"stored" (uncompressed) DEFLATE blocks**.

- **Mechanism**: Every block of pixel data is wrapped in a DEFLATE header that marks the block as uncompressed.
- **Result**: The resulting files are larger than compressed PNGs, but they are fully compliant with the PNG specification and can be opened by any standard image viewer.

## PNG Structure

The encoder constructs the file by creating the following chunks:
1. **`IHDR`**: Image header containing width, height, bit depth, and color type.
2. **`IDAT`**: The actual pixel data (RGB interleaved), wrapped in stored DEFLATE blocks.
3. **`IEND`**: The end-of-file marker.

## Core Utilities
- **CRC32**: Implements the cyclic redundancy check required for every PNG chunk.
- **Adler32**: Implemented for the zlib wrapper around the IDAT blocks.
- **`encode_rgb`**: The primary entry point that converts a raw RGB buffer into a PNG-encoded `Vec<u8>`.
