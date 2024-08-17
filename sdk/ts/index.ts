import { Connection, Keypair, Transaction, PublicKey, TransactionInstruction, TransactionInstructionCtorFields, TransactionBlockhashCtor, SendOptions, BlockhashWithExpiryBlockHeight, AccountMeta, SystemProgram, CreateAccountParams, ComputeBudgetProgram, SetComputeUnitPriceParams, SetComputeUnitLimitParams } from "@solana/web3.js";
import { sha256 } from "@noble/hashes/sha2";
import { BN } from "bn.js";

export enum ChadBufferInstruction {
    Initialize,
    Assign,
    Write,
    Close
};

export class ChadBuffer {
    programId: PublicKey = new PublicKey("bufzJtwkkoXEVh4eKFsshPFacLpgpxarasuDSvzGvxd")
    checksum: Uint8Array;
    keypair: Keypair;
    size: number;
    shards: Buffer[] = [];
    DYNAMIC_IX_SIZE = 0;
    preInstructions: TransactionInstruction[] = [];

    static INIT_DATA_SIZE = 358;
    static WRITE_DATA_SIZE = 208;
    static TX_SIZE = 1232;

    constructor(public connection: Connection, public data: Buffer, computeUnits?: { microLamports?: number | bigint, units?: number }, keypair?: Keypair) {
        this.checksum = sha256(data);
        this.keypair = keypair || new Keypair();
        this.size = data.length + 32;
        if (computeUnits?.microLamports || computeUnits?.units) {
            this.DYNAMIC_IX_SIZE += 36;
        }
        if (computeUnits?.microLamports) {
            this.preInstructions.push(ComputeBudgetProgram.setComputeUnitPrice({ microLamports: computeUnits.microLamports }));
            this.DYNAMIC_IX_SIZE += 8;
        }
        if (computeUnits?.units) {
            this.preInstructions.push(ComputeBudgetProgram.setComputeUnitLimit({ units: computeUnits.units }));
            this.DYNAMIC_IX_SIZE += 8;
        }
        this.shards = this.createShards(data);

    }

    createShards(data: Buffer): Buffer[] {
        let offset = 0;
        let shards: Buffer[] = [];

        // Allocate the first shard
        const initSize = Math.min(ChadBuffer.TX_SIZE - this.DYNAMIC_IX_SIZE - ChadBuffer.INIT_DATA_SIZE, data.length);
        shards.push(
            Buffer.concat([
                data.subarray(0, Math.min(ChadBuffer.TX_SIZE - this.DYNAMIC_IX_SIZE - ChadBuffer.INIT_DATA_SIZE, data.length))
            ])
        );
        offset += initSize;

        while (offset < data.length) {
            const shardData = data.subarray(offset, offset + Math.min(ChadBuffer.TX_SIZE - this.DYNAMIC_IX_SIZE - ChadBuffer.WRITE_DATA_SIZE, data.length));
            shards.push(
                Buffer.concat([
                    new BN(offset).toArrayLike(Buffer, 'le', 3), 
                    shardData
                ])
            );
            offset += shardData.length;
        }

        return shards
    }

    createInitializeInstruction(authority: PublicKey) {
        return new TransactionInstruction({
            programId: this.programId,
            data: Buffer.concat([
                Buffer.alloc(1, ChadBufferInstruction.Initialize), 
                this.shards[0]
            ]),
            keys: [
                {
                    pubkey: authority,
                    isSigner: true,
                    isWritable: true
                } as AccountMeta,
                {
                    pubkey: this.keypair.publicKey,
                    isSigner: true,
                    isWritable: true
                } as AccountMeta
            ]
        } as TransactionInstructionCtorFields)
    }

    createAssignInstruction(authority: PublicKey, newAuthority: PublicKey): TransactionInstruction {
        return new TransactionInstruction({
            programId: this.programId,
            data: Buffer.concat([
                Buffer.alloc(1, ChadBufferInstruction.Assign), 
                newAuthority.toBuffer()
            ]),
            keys: [
                {
                    pubkey: authority,
                    isSigner: true,
                    isWritable: true
                } as AccountMeta,
                {
                    pubkey: this.keypair.publicKey,
                    isSigner: false,
                    isWritable: true
                } as AccountMeta
            ]
        } as TransactionInstructionCtorFields)
    }
    
    createWriteInstruction(authority: PublicKey, data: Buffer): TransactionInstruction {
        return new TransactionInstruction({
            programId: this.programId,
            data: Buffer.concat([
                Buffer.alloc(1, ChadBufferInstruction.Write), 
                data
            ]),
            keys: [
                {
                    pubkey: authority,
                    isSigner: true,
                    isWritable: true
                } as AccountMeta,
                {
                    pubkey: this.keypair.publicKey,
                    isSigner: false,
                    isWritable: true
                } as AccountMeta
            ]
        } as TransactionInstructionCtorFields)
    }

    createCloseInstruction(authority: PublicKey): TransactionInstruction {
        return new TransactionInstruction({
            programId: this.programId,
            data: Buffer.alloc(1, ChadBufferInstruction.Close),
            keys: [
                {
                    pubkey: authority,
                    isSigner: true,
                    isWritable: true
                } as AccountMeta,
                {
                    pubkey: this.keypair.publicKey,
                    isSigner: false,
                    isWritable: true
                } as AccountMeta
            ]
        } as TransactionInstructionCtorFields)
    }

    async createInitializeTransaction(authority: PublicKey): Promise<Transaction> {
        let lamports = await this.connection.getMinimumBalanceForRentExemption(this.size);
        let tx = new Transaction({
            feePayer: authority,
        } as TransactionBlockhashCtor);

        if (this.preInstructions) {
            tx.add(...this.preInstructions)
        }

        tx.add(
            SystemProgram.createAccount({
                fromPubkey: authority,
                newAccountPubkey: this.keypair.publicKey,
                space: this.size,
                lamports,
                programId: this.programId // Assuming the program ID is the public key of the keypair
            } as CreateAccountParams)
        )
        tx.add(
            this.createInitializeInstruction(authority)
        )
        return tx
    }

    createAssignTransaction(authority: PublicKey, newAuthority: PublicKey): Transaction[] {
        return this.shards.slice(1).map((data) => {
            let tx = new Transaction({
                feePayer: authority,
            } as TransactionBlockhashCtor);

            if (this.preInstructions) {
                tx.add(...this.preInstructions)
            }
            
            tx.add(this.createAssignInstruction(authority, newAuthority))
            return tx
        })
    }

    createWriteTransactions(authority: PublicKey, shards?: Buffer[]): Transaction[] {
        shards = shards || this.shards.slice(1);
        return shards.map(
            (data: Buffer) => {
                let tx = new Transaction({
                    feePayer: authority,
                } as TransactionBlockhashCtor);

                if (this.preInstructions) {
                    tx.add(...this.preInstructions)
                }
                
                tx.add(this.createWriteInstruction(authority, data))
                return tx
            }
        )
    }

    createCloseTransaction(authority: PublicKey): Transaction {
        let tx = new Transaction({
            feePayer: authority,
        } as TransactionBlockhashCtor);

        if (this.preInstructions) {
            tx.add(...this.preInstructions)
        }

        tx.add(this.createCloseInstruction(authority));

        return tx;
    }
}
