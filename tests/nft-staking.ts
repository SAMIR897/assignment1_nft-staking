import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { NftStaking } from "../target/types/nft_staking";
import {
  createV1,
  createCollectionV1,
  Collection,
  Asset,
  fetchAsset,
  fetchCollection,
  pluginAuthorityPair,
} from "@metaplex-foundation/mpl-core";
import {
  createUmi,
} from "@metaplex-foundation/umi-bundle-defaults";
import {
  keypairIdentity,
  generateSigner,
  publicKey,
  transactionBuilder,
} from "@metaplex-foundation/umi";
import {
  getAssociatedTokenAddressSync,
  getAccount,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { assert } from "chai";

describe("nft-staking", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.NftStaking as Program<NftStaking>;
  const umi = createUmi(provider.connection);
  
  // Set up Umi with provider's wallet
  const user = (provider.wallet as anchor.Wallet).payer;
  const userKeypair = umi.eddsa.createKeypairFromSecretKey(user.secretKey);
  umi.use(keypairIdentity(userKeypair));

  const collectionSigner = generateSigner(umi);
  const assetSigner = generateSigner(umi);

  const [configPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId
  );

  const [rewardsMintPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("rewards"), configPda.toBuffer()],
    program.programId
  );

  const [userAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("user"), user.publicKey.toBuffer()],
    program.programId
  );

  const [stakeAccountPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("stake"), anchor.web3.PublicKey.default.toBuffer(), configPda.toBuffer()], // Will override the mint later
    program.programId
  );

  let stakeAccountActualPda: anchor.web3.PublicKey;

  it("Is initialized!", async () => {
    // 10 points per stake (per second for testing), 5 max stake, 0 freeze period for testing
    const tx = await program.methods
      .initialize(10, 5, 0)
      .accounts({
        admin: user.publicKey,
        config: configPda,
        rewardsMint: rewardsMintPda,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    console.log("Your transaction signature", tx);

    const configAccount = await program.account.stakeConfig.fetch(configPda);
    assert.equal(configAccount.pointsPerStake, 10);
    assert.equal(configAccount.maxStake, 5);
    assert.equal(configAccount.freezePeriod, 0);
  });

  it("Initializes User", async () => {
    const tx = await program.methods
      .initUser()
      .accounts({
        user: user.publicKey,
        userAccount: userAccountPda,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    console.log("Your transaction signature", tx);

    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    assert.equal(userAccount.amountStaked, 0);
    assert.equal(userAccount.points, 0);
  });

  it("Creates a Core Collection", async () => {
    await createCollectionV1(umi, {
      collection: collectionSigner,
      name: "Staking Collection",
      uri: "https://example.com/collection.json",
      plugins: [
        pluginAuthorityPair({
          type: "Attributes",
          data: {
            attributeList: [{ key: "staked_count", value: "0" }],
          },
        }),
      ],
    }).sendAndConfirm(umi);

    const collection = await fetchCollection(umi, collectionSigner.publicKey);
    assert.equal(collection.name, "Staking Collection");
  });

  it("Creates a Core Asset in Collection", async () => {
    await createV1(umi, {
      asset: assetSigner,
      name: "Staking NFT #1",
      uri: "https://example.com/nft1.json",
      collection: collectionSigner.publicKey,
    }).sendAndConfirm(umi);

    const asset = await fetchAsset(umi, assetSigner.publicKey);
    assert.equal(asset.name, "Staking NFT #1");
  });

  it("Stakes the NFT", async () => {
    stakeAccountActualPda = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("stake"), new anchor.web3.PublicKey(assetSigner.publicKey).toBuffer(), configPda.toBuffer()],
      program.programId
    )[0];

    const mplCoreProgramId = new anchor.web3.PublicKey("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

    const tx = await program.methods
      .stake()
      .accounts({
        user: user.publicKey,
        asset: new anchor.web3.PublicKey(assetSigner.publicKey),
        collection: new anchor.web3.PublicKey(collectionSigner.publicKey),
        stakeAccount: stakeAccountActualPda,
        userAccount: userAccountPda,
        config: configPda,
        mplCoreProgram: mplCoreProgramId,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    console.log("Your transaction signature", tx);

    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    assert.equal(userAccount.amountStaked, 1);

    const stakeAccount = await program.account.stakeAccount.fetch(stakeAccountActualPda);
    assert.equal(stakeAccount.owner.toBase58(), user.publicKey.toBase58());
  });

  it("Claims Rewards", async () => {
    // Wait a couple seconds to accrue rewards
    await new Promise((resolve) => setTimeout(resolve, 2000));

    const userAta = getAssociatedTokenAddressSync(
      rewardsMintPda,
      user.publicKey
    );

    const tx = await program.methods
      .claim()
      .accounts({
        user: user.publicKey,
        userAccount: userAccountPda,
        stakeAccount: stakeAccountActualPda,
        config: configPda,
        rewardsMint: rewardsMintPda,
        rewardsAta: userAta,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();
    console.log("Your transaction signature", tx);

    const tokenAccount = await getAccount(provider.connection, userAta);
    console.log("Claimed rewards balance:", tokenAccount.amount.toString());
    assert.isTrue(Number(tokenAccount.amount) > 0);
  });

  it("Unstakes the NFT", async () => {
    const userAta = getAssociatedTokenAddressSync(
      rewardsMintPda,
      user.publicKey
    );
    
    const balanceBefore = (await getAccount(provider.connection, userAta)).amount;

    await new Promise((resolve) => setTimeout(resolve, 1000));

    const mplCoreProgramId = new anchor.web3.PublicKey("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

    const tx = await program.methods
      .unstake()
      .accounts({
        user: user.publicKey,
        asset: new anchor.web3.PublicKey(assetSigner.publicKey),
        collection: new anchor.web3.PublicKey(collectionSigner.publicKey),
        stakeAccount: stakeAccountActualPda,
        userAccount: userAccountPda,
        config: configPda,
        rewardsMint: rewardsMintPda,
        rewardsAta: userAta,
        mplCoreProgram: mplCoreProgramId,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc({skipPreflight: true});
    console.log("Your transaction signature", tx);

    const userAccount = await program.account.userAccount.fetch(userAccountPda);
    assert.equal(userAccount.amountStaked, 0);

    const balanceAfter = (await getAccount(provider.connection, userAta)).amount;
    assert.isTrue(balanceAfter > balanceBefore);
  });
});
