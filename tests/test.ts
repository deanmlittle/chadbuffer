import Bluebird from 'bluebird'
import { Keypair, Connection, Transaction, SendOptions, BlockhashWithExpiryBlockHeight, ComputeBudgetProgram, SetComputeUnitPriceParams } from "@solana/web3.js"
import { randomBytes } from "crypto"
import { ChadBuffer } from "../sdk/ts"
import { BN } from "bn.js"
import { sha256 } from "@noble/hashes/sha2"

const connection = new Connection("http://127.0.0.1:8899", "processed")
const signer = Keypair.fromSecretKey(new Uint8Array(JSON.parse(process.env.SIGNER!)))

const confirm = async (signature: string): Promise<string> => {
    const block = await connection.getLatestBlockhash();
    await connection.confirmTransaction({
        signature,
        ...block,
    });
    return signature;
};

const log = async (signature: string): Promise<string> => {
    console.log(`Transaction successful! https://explorer.solana.com/tx/${signature}?cluster=custom&customUrl=${connection.rpcEndpoint}`);
    return signature;
};

const signAndSendWithBlockhash = async(tx: Transaction, signers: Keypair[], options?: SendOptions, block?: BlockhashWithExpiryBlockHeight): Promise<string> => {
    block = block || await connection.getLatestBlockhash();
    tx.recentBlockhash = block.blockhash;
    tx.lastValidBlockHeight = block.lastValidBlockHeight;
    const signature = await connection.sendTransaction(tx, signers, options);
    return signature;
};

const batchProcess = async (chadbuf: ChadBuffer, signer: Keypair, batchSize = 20, block?: BlockhashWithExpiryBlockHeight) => {
    let counter = 1
    let txs = chadbuf.createWriteTransactions(signer.publicKey)

    const broadcastAndConfirm = async (txs: Transaction[], signer: Keypair, batchSize: number, block?: BlockhashWithExpiryBlockHeight) => {
        block = block || await connection.getLatestBlockhash();
        await Bluebird.map(
            txs,
            async (tx: Transaction) => {
                try {
                    await signAndSendWithBlockhash(tx, [signer], { skipPreflight: true }, block)
                        .then(() => {
                            counter++;
                            console.log(`${counter}/${txs.length+1} Confirmed`);
                        });
                } catch (error) {
                    console.error("Error processing transaction:", error);
                    throw error;  // Optional: Decide if you want to stop processing on error or continue
                }
            },
            { concurrency: batchSize }
        );

        // Verify end state
        console.log("Validating state")
        const account = await connection.getAccountInfo(chadbuf.keypair.publicKey);
        const accountData = account!.data!.slice(32)
        txs = txs.filter((tx) => {
            const shard = tx.instructions[tx.instructions.length-1].data;
            const offset = new BN(shard.subarray(1,4), undefined, "le").toNumber();
            const data = shard.subarray(4);
            return Buffer.compare(accountData.subarray(offset, offset+data.length), data) != 0
        })

        if (txs.length > 0) {
            // Don't include block as we want it to update
            console.log(`Self-healing ${txs.length} transactions`)
            counter = 0;
            await broadcastAndConfirm(txs, signer, batchSize)
        }
    }

    // Broadcast all write TXs
    await broadcastAndConfirm(txs, signer, batchSize, block);

    console.log("All transactions confirmed");
};

describe('ChadBuffer tests', function() {
    this.timeout(120000);
    const length = 3000000;
    const data = randomBytes(length);
    const chadbuf = new ChadBuffer(connection, data, { microLamports: 200000, units: 1000 });
    let block: BlockhashWithExpiryBlockHeight;

    it('Initialize a ChadBuffer', async () => {
        block = await connection.getLatestBlockhash();
        let tx = await chadbuf.createInitializeTransaction(signer.publicKey);
        await signAndSendWithBlockhash(tx, [signer, chadbuf.keypair], undefined, block).then((signature) => {
            console.log(`1/${chadbuf.shards.length} Confirmed`);
            return signature
        }).then(log);
    });

    it('Batch write ChadBuffer data', async () => {
        await batchProcess(chadbuf, signer, 100, block);
    });

    it('Close a ChadBuffer', async () => {
        await setTimeout(()=>{console.log('Wait')},3000); 
        let account = await connection.getAccountInfo(chadbuf.keypair.publicKey, connection.commitment);
        let data = account!.data.subarray(32);
        let hash = sha256(data);

        if (Buffer.compare(chadbuf.checksum, hash) !== 0) {
            throw new Error("Hash mismatch");
        }

        await signAndSendWithBlockhash(chadbuf.createCloseTransaction(signer.publicKey), [signer], {  skipPreflight: true }, block)
            .then(confirm)
            .then(log);
    });
});
