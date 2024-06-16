import assert from "assert";
import * as anchor from "@coral-xyz/anchor";
import { 
  clusterApiUrl,
  PublicKey,
  Keypair,
  LAMPORTS_PER_SOL,
  Connection,
  SystemProgram
} from "@solana/web3.js";

import { 
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  transfer,
  Account,
  getMint,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

describe("solana_amm", () => {
  // Establish connection and provider
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);
  const connection = new Connection(clusterApiUrl("devnet"), 'confirmed');
  const program = anchor.workspace.SolanaAmm;

  let amm: PublicKey;
  let userTokenAccount: Account;
  let ammTokenAccount: Account;
  let mint: PublicKey;
  const fromWallet = Keypair.generate();
  const toWallet = new PublicKey("G2k6ShTNEyJo84Gu6Dey6ubKagaFQzjBxffncNtPJuqR");

  it("Initialize AMM and Mint Token", async () => {
    // Airdrop SOL to the wallet
    const fromAirdropSignature = await connection.requestAirdrop(fromWallet.publicKey, LAMPORTS_PER_SOL);
    await connection.confirmTransaction(fromAirdropSignature);

    // Create new token mint
    mint = await createMint(
      connection,
      fromWallet,
      fromWallet.publicKey,
      null, 
      9
    );
    console.log(`Created token: ${mint.toBase58()}`);

    // Create associated token accounts
    userTokenAccount = await getOrCreateAssociatedTokenAccount(
      connection,
      fromWallet,
      mint,
      fromWallet.publicKey
    );
    ammTokenAccount = await getOrCreateAssociatedTokenAccount(
      connection,
      fromWallet,
      mint,
      fromWallet.publicKey
    );

    console.log(`Created Token Account for User: ${userTokenAccount.address.toBase58()}`);
    console.log(`Created Token Account for AMM: ${ammTokenAccount.address.toBase58()}`);

    // Mint some tokens to the user's token account
    await mintTo(
      connection,
      fromWallet,
      mint,
      userTokenAccount.address,
      fromWallet.publicKey,
      10000000000
    );
    console.log(`Minted tokens to user account`);

    // Initialize the AMM
    const [ammPda, bump] = await PublicKey.findProgramAddress(
      [Buffer.from("amm")],
      program.programId
    );

    await program.rpc.initialize({
      accounts: {
        amm: ammPda,
        user: provider.wallet.publicKey,
        systemProgram: SystemProgram.programId,
      },
    });

    amm = ammPda;
    const ammAccount = await program.account.amm.fetch(ammPda);
    assert.equal(ammAccount.reserveA.toString(), "0");
    assert.equal(ammAccount.reserveSol.toString(), "0");
  });

  it("Add Liquidity", async () => {
    const amountA = new anchor.BN(1000000); // 1 token with 6 decimals
    const solAmount = new anchor.BN(1000000000); // 1 SOL

    await program.rpc.addLiquidity(amountA, solAmount, {
      accounts: {
        amm,
        user: provider.wallet.publicKey,
        userTokenA: userTokenAccount.address,
        ammTokenA: ammTokenAccount.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      },
    });

    const ammAccount = await program.account.amm.fetch(amm);
    assert.equal(ammAccount.reserveA.toString(), amountA.toString());
    assert.equal(ammAccount.reserveSol.toString(), solAmount.toString());
  });

  it("Remove Liquidity", async () => {
    const amountA = new anchor.BN(500000); // 0.5 tokens with 6 decimals

    await program.rpc.removeLiquidity(amountA, {
      accounts: {
        amm,
        user: provider.wallet.publicKey,
        userTokenA: userTokenAccount.address,
        ammTokenA: ammTokenAccount.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      },
    });

    const ammAccount = await program.account.amm.fetch(amm);
    assert.equal(ammAccount.reserveA.toString(), "500000");
    assert.equal(ammAccount.reserveSol.toString(), "500000000");
  });

  it("Buy Tokens", async () => {
    const solAmount = new anchor.BN(100000000); // 0.1 SOL

    await program.rpc.buy(solAmount, {
      accounts: {
        amm,
        user: provider.wallet.publicKey,
        userTokenA: userTokenAccount.address,
        ammTokenA: ammTokenAccount.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      },
    });

    const ammAccount = await program.account.amm.fetch(amm);
    assert.equal(ammAccount.reserveSol.toString(), "600000000");
    // Token amount will be decreased according to AMM logic
  });

  it("Sell Tokens", async () => {
    const amountA = new anchor.BN(100000); // 0.1 tokens with 6 decimals

    await program.rpc.sell(amountA, {
      accounts: {
        amm,
        user: provider.wallet.publicKey,
        userTokenA: userTokenAccount.address,
        ammTokenA: ammTokenAccount.address,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      },
    });

    const ammAccount = await program.account.amm.fetch(amm);
    assert.equal(ammAccount.reserveA.toString(), "600000");
    // SOL amount will be decreased according to AMM logic
  });
});
