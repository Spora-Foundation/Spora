# Testnet 11

*Testnet 11* is the test network running the first public 10BPS Spora experiment. The goal of this document is to provide a quick guide for anyone who wants to participate.

In the future testnet 11 will act as a staging zone for all sorts of experiments, allowing us to stress test various approaches and ideas on a global scale with the participation of the community. The approaches we decide to adopt will then be stress tested for longer periods on testnet 10 before being incorporated into the mainnet.

## Overview

On the software side, participating requires three components:
1. *Sporad* - the Spora client
2.  *spora-miner* - the Spora CPU miner
3.  *Treasure Boy* - a transaction generator

The Treasure Boy tool is used to create a wallet, and once the wallet has some funds within, Treasure Boy will continuously create transactions from that wallet back to itself at the prescribed rate.

The Treasure Boy wallet could be funded by either mining to it directly (either for a short period or continuously) or by asking other experiment participants for some funds (e.g. on the Discord \#testnet channel).

## How to participate

The only prerequisite is running a node that's connected to Testnet 11. Other than that you produce blocks by running a miner, produce transactions by running Treasure Boy, or both.

Since we want the test condition to be as close as possible to organic, we encourage users to diversify their roles, and the hardware they use to participate.

The venue for discussing and monitoring the experiment will be the \#testnet channel on Discord. We encourage participants to describe the experience in general, and also tell us what hardware they are using and how well it handles the load.

The recommended hardware requirements are 16GB of RAM, preferably a CPU with at least 8 cores, and an SSD drive with at least 250GB of free space (the 300GB for a safety margin is preferable). It might also be possible to operate with 8GB of RAM, by passing the `--ram-scale=0.6` parameter to `Sporad`.

## Setup Instructions

Testnet11 uses a dedicated P2P port (16311) so that nodes from the usual testnet (10) do not automatically attempt connecting to it.

We reiterate that only the included miner should be used to maintain a level playing field.

First, we set-up a node:
1. Download and extract the [spora binaries](https://github.com/AvatoLabs/Spora/releases). Alternatively, you can compile it from source yourself by following the instructions [here](https://github.com/AvatoLabs/Spora/blob/master/README.md). The rest of the instructions are written assuming the former option. If you choose to locally compile the code, replace any command of the form ``<program> <arguments>`` with ``cargo run --bin <program> --release -- <arguments>`` (see example in the next item). All actions described below should be performed on a command line window where you navigated to the directory into which the binaries were extracted.
2. Start the ``Sporad`` client with ``utxoindex`` enabled:

```
Sporad --testnet --netsuffix=11 --utxoindex
```
  It is **very important** not to forget the ``--netsuffix=11`` flag, otherwise your node will connect to mainnet or to the default 1 BPS testnet.
  If you complied the code yourself, you should instead run
```
cargo run --bin Sporad --release -- --testnet --netsuffix=11 --utxoindex
```
  Leave this window open, there is no need to touch it as long as the node is running. Closing it will stop the node.

If you want to transmit transactions, first create a Treasure Boy wallet
1. Run ``treasure_boy`` to generate a wallet
2. The output will provide you with a private key (that looks like a bunch of gibberish) and a public address (that looks like "tondi0:" followed by a bunch of gibberish). For example, the output could look like this:
     ```
     2023-06-25 18:00:58.677+00:00 [INFO ] Connected to RPC
     2023-06-25 18:00:58.677+00:00 [INFO ] Generated private key aa1c554386218eb28c4bsf6a02e5943799cf951dac7301324d88dec2d0119fce and address tondi0:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvp55hu9. Send some funds to this address and rerun treasure_boy with `--private-key aa1c554386218eb28c4bsf6a02e5943799cf951dac7301324d88dec2d0119fce`
      ```
     Here, the private key is ```aa1c554386218eb28c4bsf6a02e5943799cf951dac7301324d88dec2d0119fce``` and the address is ```tondi0:qp6hs9tjpfe6e4dpvtpj5wvt3l77c562fnk5g3wuxvpjz5xsfluhvp55hu9```
3. Put some money into the wallet. This could be done by either mining to that wallet (see below) or asking other participants to send money to your public address in the \#testnet Discord channel.
4. Once the wallet has been funded, run Treasure Boy with the private key:
   ```
   treasure_boy --private-key <private-key> -t=50
   ```
  The last parameter ``-t=50`` means Treasure Boy will attempt broadcasting 50 transactions per second. We encourage participants to run with different TPS values. However, in order to encourage transaction spread and to simulate organic usage we highly recommend not to go above 100 TPS.

Like Sporad, the Treasure Boy window should remain open and undisturbed.

For mining, grab `spora-miner` from within the latest [Release](https://github.com/elichai/spora-miner/releases) and run it with the following flags (**this is currently the only miner that supports testnet-11**):
    ```
    spora-miner --testnet --mining-address <address> -p 16210 -t 1
    ```

If you intend to run Treasure Boy, replace ``<address>`` with the address of the wallet generated by ``treasure_boy``, you should then wait for a while before your wallet accumulates enough funds. Assuming several dozen participants, 20 minutes should be more than enough. If you just mine for the sake of mining, you could use any address, such as the one provided in the example above.

Like the Sporad and Treasure Boy windows, the miner window should also be left undisturbed, and closing it will stop the mining.
