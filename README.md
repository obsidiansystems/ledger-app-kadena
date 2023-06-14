# Kadena Ledger App

[Nix]: https://nixos.org/

## Device Compatability

This application is compatible with
- Ledger Nano S, running firmware 2.1.0 and above
- Ledger Nano S+, running firmware 1.1.0
- Ledger Nano X

Note: Compatibility with Ledger Nano X is only possible to check on [Speculos](https://github.com/ledgerHQ/speculos/) emulator,
because the Nano X does not support side-loading apps under development.

## Preparing Your Linux Machine for Ledger Device Communication

On Linux, the "udev" rules must be set up to allow your user to communicate with the ledger device. MacOS devices do not need any configuration to communicate with a Ledger device, so if you are using Mac you can ignore this section.

### macOS

No steps need to be taken in advance.

### NixOS

On NixOS, one can easily do this with by adding the following to configuration.nix:

``` nix
{
  # ...
  hardware.ledger.enable = true;
  # ...
}
```

### Non-NixOS Linux Distros

For non-NixOS Linux distros, LedgerHQ provides a [script](https://raw.githubusercontent.com/LedgerHQ/udev-rules/master/add_udev_rules.sh) for this purpose, in its own [specialized repo](https://github.com/LedgerHQ/udev-rules). Download this script, read it, customize it, and run it as root:

```shell
wget https://raw.githubusercontent.com/LedgerHQ/udev-rules/master/add_udev_rules.sh
chmod +x add_udev_rules.sh
```

**We recommend against running the next command without reviewing the script** and modifying it to match your configuration.

```shell
sudo ./add_udev_rules.sh
```

Subsequently, unplug your ledger hardware wallet, and plug it in again for the changes to take effect.

For more details, see [Ledger's documentation](https://support.ledger.com/hc/en-us/articles/115005165269-Fix-connection-issues).

## Installing the app

If you don't want to develop the app but just use it, installation should be very simple.
The first step is to obtain a release tarball.
The second step is to load that app from the tarball.

Additionaly, if you are using [Nix], you can skip the tarball entirely and directly build/downoad and load the load.

### Directly build/download and load the app with Nix

First, follow our [general instructions](./NIX.md) for getting started with [Nix].

Second, please ensure that your device is plugged, unlocked, and on the device home screen.

Finally, run the following command to load the app on your device:
```bash
nix --extra-experimental-features nix-command run -f . $DEVICE.loadApp
```
where `DEVICE` is one of
 - `nanos`, for Nano S
 - `nanox`, for Nano X
 - `nanosplus`, for Nano S+

The app will be downloaded (if you have our Nix cache enabled) and/or freshly built as needed.

### Obtaining a release tarball

#### Download an official build

Check the [releases page](https://github.com/obsidiansystems/ledger-app-kadena/releases) of this app to see if an official build has been uploaded for this release.
There is a separate tarball for each device.

#### Build one yourself, with Nix

First, follow our [general instructions](./NIX.md) for getting started with [Nix].

There is a separate tarball for each device.
To build one, run:
```bash
nix-build -A $DEVICE.tarball
```
where `DEVICE` is one of
 - `nanos`, for Nano S
 - `nanox`, for Nano X
 - `nanosplus`, for Nano S+

The last line printed out will be the path of the tarball.

### Installation using the pre-packaged tarball

Before installing please ensure that your device is plugged, unlocked, and on the device home screen.

#### With Nix

By using Nix, this can be done simply by using the `load-app` command, without manually installing the `ledgerctl` on your system.

```bash
tar xzf /path/to/release.tar.gz
cd kadena-$DEVICE
nix-shell
load-app
```

`/path/to/release.tar.gz` you should replace with the actual path to the tarball.
For example, it might be `~/Downloads/release.tar.gz` if you downloaded a pre-built official release from GitHub, or `/nix/store/adsfijadslifjaslif-release.tar.gz` if you built it yourself with Nix.

#### Without Nix

Without using Nix, the [`ledgerctl`](https://github.com/LedgerHQ/ledgerctl) can be used directly to install the app with the following commands.
For more information on how to install and use that tool see the [instructions from LedgerHQ](https://github.com/LedgerHQ/ledgerctl).

```bash
tar xzf release.tar.gz
cd kadena-$DEVICE
ledgerctl install -f app.json
```

## Using the app with generic CLI tool

The bundled [`generic-cli`](https://github.com/alamgu/alamgu-generic-cli) tool can be used to obtaining the public key and do signing.

To use this tool using Nix, from the root level of this repo, run this command to enter a shell with all the tools you'll need:
```bash
nix-shell -A $DEVICE.appShell
```
where `DEVICE` is one of
 - `nanos`, for Nano S
 - `nanox`, for Nano X
 - `nanosplus`, for Nano S+

Then, one can use `generic-cli` like this:

- Get a public key for a BIP-32 derivation without prompting the user:
  ```shell-session
  $ generic-cli getAddress "44'/626'/0'/0/0"
  a42e71c004770d1a48956090248a8d7d86ee02726b5aab2a5cd15ca9f57cbd71
  ```

- Show the address on device for a BIP-32 derivation and obtain the public key:
  ```shell-session
  $ generic-cli getAddress --verify "44'/626'/0'/0/0"
  a42e71c004770d1a48956090248a8d7d86ee02726b5aab2a5cd15ca9f57cbd71
  ```

- Sign a transaction:
  ```shell-session
  $ generic-cli sign --json "44'/626'/0'/0/0" '{"networkId":"mainnet01","payload":{"exec":{"data":{"ks":{"pred":"keys-all","keys":["368820f80c324bbc7c2b0610688a7da43e39f91d118732671cd9c7500ff43cca"]}},"code":"(coin.transfer-create \"alice\" \"bob\" (read-keyset \"ks\") 100.1)\n(coin.transfer \"bob\" \"alice\" 0.1)"}},"signers":[{"pubKey":"6be2f485a7af75fedb4b7f153a903f7e6000ca4aa501179c91a2450b777bd2a7","clist":[{"args":["alice","bob",100.1],"name":"coin.TRANSFER"},{"args":[],"name":"coin.GAS"}]},{"pubKey":"368820f80c324bbc7c2b0610688a7da43e39f91d118732671cd9c7500ff43cca","clist":[{"args":["bob","alice",0.1],"name":"coin.TRANSFER"}]}],"meta":{"creationTime":1580316382,"ttl":7200,"gasLimit":1200,"chainId":"0","gasPrice":1.0e-5,"sender":"alice"},"nonce":"2020-01-29 16:46:22.916695 UTC"}'
  ```

Alternatively the contents of JSON could be copied to a file, and the name of the file could be used in the command-line instead. This is necessary when the size of the JSON being signed is very big, as the command-line has limits to the length.

The following command demonstrates signing a big transaction specified in the file `./ts-tests/marmalade-tx.json`

  ```shell-session
  $ generic-cli sign --file --json "44'/626'/0'/0/0" ./ts-tests/marmalade-tx.json
  ```

## Development

See [CONTRIBUTING.md](./CONTRIBUTING.md).
