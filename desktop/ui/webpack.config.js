const path = require("path");

module.exports = {
    entry: "./build/tmp/index.js",
    mode: "development",
    output: {
        path: path.resolve(__dirname, "build"),
        filename: "index.js",
    },
};
