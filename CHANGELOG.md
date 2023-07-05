## 0.2.5

* Revert the "Max Gas" calculation.
* Minor changes to Gas prompts of MAKE_TRANSFER_TX APDU.

## 0.2.4

* Minor changes to the prompts of MAKE_TRANSFER_TX APDU.
* Bump nanos_sdk

## 0.2.3

* Add VERIFY_ADDRESS APDU, which prompts user showing the address.
* GET_PUBKEY APDU now does not prompt the user for confirmation.
* Improvements to the menu; show icons.
* Fix crash when incorrect BIP32 path is provided.
* Support for NanosS+ FW 1.1.0.
* Fixes and upgrades to the build infrastructure.

## 0.2.2

* Changes to the prompts for the transfer transaction signing flow. Show the sender, recipient and amount in separate prompts and do pagination of prompts.
* Fix for a potential stack overflow issue on NanoS device which could occur while converting the transaction hash to base64 format for displaying.
* Miscelaneous fixes and upgrades to the build infrastructure and Github CI actions.

## 0.2.1

Added support for the following

* Added support for building and signing transfer, transfer-create, and transfer-crosschain transactions on the Ledger device.
* Signing arbitrary hash. This must be enabled from the 'Settings' menu.
* Signing transaction containing arbitrary number of capabilities, with arbitrary number of arguments.
* A 'Cross-transfer' message is displayed for 'coin.TRANSFER_XCHAIN' capability.
* A generic message will be shown for unknown capabilities, with upto five arguments.
  And a warning will be shown if the arguments of the capability cannot be shown on the Ledger device.
* 'networkId' and 'clist' fields can be 'null'.
* The 'Transfer' and 'Unknown Capability' prompts shows the index.

## 0.1.0

* Initial version with support for signing simple 'coin.TRANSFER', 'coin.ROTATE' and 'coin.transfer-crosschain' transactions.
