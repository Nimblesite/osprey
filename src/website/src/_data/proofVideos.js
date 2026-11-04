const videos = require("./videos.json");

module.exports = Object.entries(videos)
  .filter(([, entry]) => entry.proofTopic)
  .map(([id, entry]) => ({ id, ...entry }));
