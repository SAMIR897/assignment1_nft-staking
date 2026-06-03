import { updatePluginV1, pluginAuthorityPair } from "@metaplex-foundation/mpl-core";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import { keypairIdentity, generateSigner } from "@metaplex-foundation/umi";
const umi = createUmi("http://127.0.0.1:8899");
const kp = generateSigner(umi);
umi.use(keypairIdentity(kp));
const coll = generateSigner(umi);
const tx = updatePluginV1(umi, {
  asset: coll.publicKey,
  plugin: {
    __kind: "Attributes",
    attributeList: [{key: "a", value: "b"}]
  }
});
const keys = tx.getInstructions()[0].keys;
console.log(keys.map(k => ({ pubkey: k.pubkey, isSigner: k.isSigner, isWritable: k.isWritable })));
