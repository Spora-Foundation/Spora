// Run with: node demo.js
// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Generator,
    RpcClient,
    sporaToSau,
    initConsolePanicHook
} = require('../../../../nodejs/spora');

initConsolePanicHook();

const { encoding, networkId } = require("../utils").parseArgs();

(async () => {

    // console.log("using destination address:", destinationAddress);

    // From BIP0340
    const privateKey = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef');

    const sourceAddress = privateKey.toKeypair().toAddress(networkId);
    console.info(`Full spora address: ${sourceAddress}`);

    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }

    let { entries } = await rpc.getCellsByAddresses([sourceAddress]);

    if (!entries.length) {
        console.error(`No cells found for address ${sourceAddress}`);
    } else {
        console.info(entries);

        // a very basic JS-driven cell entry sort
        entries.sort((a, b) => a.amount > b.amount ? 1 : -1);

        // create a transaction generator
        // entries: an array of CellEntry
        // outputs: an array of [address, amount]
        //
        // priorityFee: a priorityFee value in Sau
        // NOTE: The priorityFee applies only to the final transaction
        //
        // changeAddress: a change address
        //
        // NOTE: the Generator iterate over the entries array
        // and create transactions until the requested amount
        // is reached. The remaining amount will be sent 
        // to the change address.
        //
        // If the requested amount is greater than the Spora
        // transaction mass, the Generator will create multiple
        // transactions where each transaction will forward
        // cells to the change address, until the requested
        // amount is reached.  It will then create a final
        // transaction according to the supplied outputs.
        let generator = new Generator({
            entries,
            outputs: [{ address : sourceAddress, amount : sporaToSau(0.2)}],
            priorityFee: sporaToSau(0.0001),
            changeAddress: sourceAddress,
        });

        // provides a generator summary by simulating
        // transaction creation and returning the
        // `GeneratorSummary` object
        let estimate = await generator.estimate();
        console.log(estimate);

    }

    await rpc.disconnect();

})();
