# chadBuffer

ChadBuffer is a Solana program that allows you to manage and manipulate permissioned data buffers on the Solana blockchain in parallel. The program provides instructions to initialize a buffer, write data to the buffer at an offset, assign ownership to another address, and close the buffer. This is useful for applications that require storing and managing large amounts of data across multiple transactions.

## Features

- **Initialize**: Create a new buffer account and initialize it with data.
- **Assign**: Assign buffer authority to another address.
- **Write**: Write additional data to an existing buffer.
- **Close**: Finalize and close the buffer, ensuring all data is written correctly.

## Installation

To use ChadBuffer, you'll need to have the following installed:

- [Node.js](https://nodejs.org/)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools)
- [Yarn](https://yarnpkg.com/getting-started/install)

### Clone the Repository

```bash
git clone https://github.com/deanmlittle/chadbuffer.git
cd chadbuffer
```

### Install Dependencies

```bash
yarn install
```

## Configuration

Before running the program, ensure you have the necessary environment variables set up:

- `SIGNER`: The path to the JSON file containing the signer's keypair.
- `RPC_URL`: The RPC URL for your Solana cluster (e.g., `http://127.0.0.1:8899` for local development).

Example:

```bash
export SIGNER=~/.config/solana/id.json
export RPC_URL=http://127.0.0.1:8899
```

## Usage

### Initialize a Buffer

To initialize a new buffer:

```typescript
import { ChadBuffer, signAndSendWithBlockhash, connection, confirm, log } from './sdk';

const data = new Uint8Array([/* your data here */]);
const chadBuffer = new ChadBuffer(data);

async function initializeBuffer() {
    const txs = await chadBuffer.init_ixs(signer);
    await signAndSendWithBlockhash(txs[0], [signer, chadBuffer.keypair])
        .then(confirm)
        .then(log);
}

initializeBuffer().catch(console.error);
```

### Write Data to a Buffer

To write additional data to an existing buffer:

```typescript
async function writeBufferData() {
    await batchProcess(txs.slice(1), signer, 100);
}

writeBufferData().catch(console.error);
```

### Close a Buffer

To close and finalize a buffer:

```typescript
async function closeBuffer() {
    let account = await connection.getAccountInfo(chadBuffer.keypair.publicKey);
    let data = account!.data.subarray(32);
    let hashed = sha256(data);

    if (Buffer.compare(chadBuffer.hash, hashed) !== 0) {
        throw new Error("Hash mismatch");
    }

    await signAndSendWithBlockhash(chadBuffer.close_ix(signer), [signer])
        .then(confirm)
        .then(log);
}

closeBuffer().catch(console.error);
```

## Testing

The ChadBuffer program includes a suite of tests to ensure the correctness of the buffer operations. To run the tests:

```bash
yarn test
```

This will execute a series of Mocha tests that validate the initialization, writing, and closing operations.

## Troubleshooting

### Common Issues

- **Blockhash Expiry**: If you encounter issues with transactions failing due to blockhash expiry, ensure you're refreshing the blockhash before processing large batches of transactions.
- **Invalid Signer**: Ensure that the signer keypair is correctly configured and that the signer is authorized to perform the operations.

### Debugging

If you run into issues, check the console output for detailed error messages. You can also add additional logging to the transaction processing steps to better understand where things might be going wrong.

## Contributing

Contributions to ChadBuffer are welcome! If you have any ideas for improvements or have found bugs, feel free to open an issue or submit a pull request.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.