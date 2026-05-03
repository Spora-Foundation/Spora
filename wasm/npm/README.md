# Spora WASM SDK

An integration wrapper around [`spora-wasm`](https://www.npmjs.com/package/spora-wasm) module that uses [`websocket`](https://www.npmjs.com/package/websocket) W3C adaptor for WebSocket communication.

This is a Node.js module that provides bindings to the Spora WASM SDK strictly for use in the Node.js environment. The web browser version of the SDK is available as part of official SDK releases at [https://github.com/AvatoLabs/Spora/releases](https://github.com/AvatoLabs/Spora/releases)

## Usage

Spora NPM module exports include all WASM32 bindings.
```javascript
const spora = require('spora');
console.log(spora.version());
```

## Documentation

Documentation is available at [https://spora.aspectron.org/docs/](https://spora.aspectron.org/docs/)


## Building from source & Examples

SDK examples as well as information on building the project from source can be found at [https://github.com/AvatoLabs/Spora/tree/master/wasm](https://github.com/AvatoLabs/Spora/tree/master/wasm)

## Releases

Official releases as well as releases for Web Browsers are available at [https://github.com/AvatoLabs/Spora/releases](https://github.com/AvatoLabs/Spora/releases).

Nightly / developer builds are available at: [https://aspectron.org/en/projects/spora-wasm.html](https://aspectron.org/en/projects/spora-wasm.html)

