const express = require("express");
const path = require("path");

const app = express();
const PORT = process.env.PORT || 3000;

app.listen(PORT, () => console.log(`Listening on port ${PORT}`));

app.use(express.static(path.join(__dirname, "dist")));
app.get("*", (req, res) => {
  res.sendFile(path.join(__dirname, "dist/index.html"));
});
