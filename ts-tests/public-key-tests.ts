import { sendCommandAndAccept, BASE_URL, } from "./common";
import { expect } from 'chai';
import { describe, it } from 'mocha';
import Axios from 'axios';
import Kda from "hw-app-kda";

describe('public key tests', () => {

  afterEach( async function() {
    await Axios.post(BASE_URL + "/automation", {version: 1, rules: []});
    await Axios.delete(BASE_URL + "/events");
  });

  it('provides a public key', async () => {

    await sendCommandAndAccept(async (client : Kda) => {
      const rv = await client.getPublicKey("44'/626'/0");
      expect(new Buffer(rv.publicKey).toString('hex')).to.equal("3f6f820616c6d999667deca91a0eccf25f62e2c910a4e77e811241445db888d7");
      return;
    }, []);
  });

  it('does address verification', async () => {

    await sendCommandAndAccept(async (client : Kda) => {
      const rv = await client.verifyAddress("44'/626'/1");
      expect(new Buffer(rv.publicKey).toString('hex')).to.equal("10f26b7f3a51d6b9ebbff3a58a5b79fcdef154cbb1fb865af2ee55089a2a1d4f");
      return;
    }, [
      {
        "header": "Provide Public Key",
        "prompt": "",
      },
      {
        "header": "Address",
        "prompt": "k:10f26b7f3a51d6b9ebbff3a58a5b79fcdef154cbb1fb865af2ee55089a2a1d4f",
        "paginate": true,
      },
      {
        "text": "Confirm",
        "x": 43,
        "y": 11,
      },
    ]);
  });
});
