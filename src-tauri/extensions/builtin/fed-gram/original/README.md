# 🗃️ FED-GRAM

A lightweight tool to download images from public Instagram posts, built with Python and Streamlit.

## Features

- Paste a public Instagram post URL and preview the image
- Supports carousel posts (multiple images)
- One-click download to your device

## Run locally

```bash
git clone https://github.com/FED-OS/FED-GRAM.git
cd FED-GRAM
pip install -r requirements.txt
streamlit run app.py
```

## Deploy for free (Streamlit Community Cloud)

1. Push this repo to GitHub.
2. Go to [share.streamlit.io](https://share.streamlit.io) and sign in with GitHub.
3. Click **New app**, select this repo and branch, set the main file to `app.py`.
4. Click **Deploy**. Your app goes live at `your-app-name.streamlit.app`.

## Limitations

- Only works on **public** posts — private accounts will fail.
- Video/Reel downloads are not supported (images only).
- Instagram may rate-limit or block scraping if used heavily; this is intended for light, personal use.

## License

MIT
