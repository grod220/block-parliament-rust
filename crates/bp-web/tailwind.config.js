/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.rs", "./index.html"],
  theme: {
    extend: {
      fontFamily: {
        sans: ["monospace"],
        mono: ["monospace"],
      },
    },
  },
  plugins: [],
};
