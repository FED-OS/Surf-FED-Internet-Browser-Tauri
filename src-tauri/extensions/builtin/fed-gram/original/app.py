import streamlit as st
import instaloader
import requests

st.set_page_config(
    page_title="FED-GRAM",
    page_icon="🖼️",
    layout="centered"
)

st.title("🗃️ FED-GRAM")
st.caption("Download images from public Instagram posts")
st.write("---")

url = st.text_input(
    "Instagram post URL:",
    placeholder="https://www.instagram.com/p/XXXXXXXXXXX/"
)

if st.button("Extract Image", type="primary"):
    if url and "/p/" in url:
        with st.spinner("Fetching image..."):
            try:
                L = instaloader.Instaloader(
                    download_videos=False,
                    download_comments=False,
                    download_geotags=False,
                    save_metadata=False,
                    quiet=True,
                )

                shortcode = url.split("/p/")[1].split("/")[0]
                post = instaloader.Post.from_shortcode(L.context, shortcode)

                if post.mediacount > 1:
                    st.subheader(f"📸 Carousel post ({post.mediacount} images)")
                    for i, node in enumerate(post.get_sidecar_nodes()):
                        st.image(node.display_url, use_container_width=True)
                        img_data = requests.get(node.display_url).content
                        st.download_button(
                            label=f"💾 Download image {i + 1}",
                            data=img_data,
                            file_name=f"fedgram_{shortcode}_{i + 1}.jpg",
                            mime="image/jpeg",
                            key=f"dl_{i}",
                        )
                else:
                    st.image(post.url, caption=shortcode, use_container_width=True)
                    img_data = requests.get(post.url).content
                    st.download_button(
                        label="💾 Download Image",
                        data=img_data,
                        file_name=f"fedgram_{shortcode}.jpg",
                        mime="image/jpeg",
                        use_container_width=True,
                    )

                st.success("Done.")

            except instaloader.exceptions.ConnectionException:
                st.error("Couldn't reach Instagram. Try again in a moment (rate limited).")
            except Exception:
                st.error("Failed. The post may be private, deleted, or the link is invalid.")
    elif url:
        st.warning("Paste a valid Instagram post URL containing '/p/'")
    else:
        st.info("Paste a link above to get started.")

st.write("---")
st.caption("⚠️ Only works on public posts. For personal use only.")
