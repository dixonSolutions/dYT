#!/usr/bin/env bash
# A small LGPL ffmpeg for dYT. The app only ever demuxes, cuts and remuxes with `-c copy`, so
# this build has no encoders, no decoders, no network and no device access: the MP4,
# WebM/Matroska and Ogg (de)muxers, the parsers and bitstream filters stream copy needs, and
# file I/O. A few MB instead of ~80.
#
#   packaging/ffmpeg/build.sh <target> <out-dir>
#
# target: linux (the host), or android-arm64-v8a | android-armeabi-v7a | android-x86_64
# (needs ANDROID_NDK_HOME). Android builds are named libffmpeg.so so the APK installs them,
# executable, into the app's native-library directory, where the engine looks for it.
set -euo pipefail

VERSION=9.0.2
TARGET=${1:?target}
OUT=$(realpath -m "${2:?out dir}")
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=${FFMPEG_WORK:-$HERE/../../target/ffmpeg}
API=${ANDROID_API:-26}
JOBS=${JOBS:-$(nproc)}

mkdir -p "$WORK" "$OUT"
SRC=$WORK/ffmpeg-$VERSION
if [ ! -d "$SRC" ]; then
	tarball=$WORK/ffmpeg-$VERSION.tar.xz
	fetch() { curl -fL --retry 5 --retry-all-errors --connect-timeout 20 -o "$1" "$2"; }
	if [ ! -f "$tarball" ] && ! fetch "$tarball" "https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"; then
		rm -f "$tarball"
		# ffmpeg.org is often slow or resets connections: the same release tag from GitHub's mirror.
		gh=$WORK/ffmpeg-$VERSION-github.tar.gz
		fetch "$gh" "https://github.com/FFmpeg/FFmpeg/archive/refs/tags/n$VERSION.tar.gz"
		echo "ffmpeg source from the GitHub mirror: sha256 $(sha256sum "$gh" | cut -d' ' -f1)" >&2
		tar -xzf "$gh" -C "$WORK"
		mv "$WORK/FFmpeg-n$VERSION" "$SRC"
	fi
	if [ ! -d "$SRC" ]; then
		# Pinned in packaging/ffmpeg/SHA256SUMS once known; a mismatch stops the build.
		if grep -q "ffmpeg-$VERSION.tar.xz" "$HERE/SHA256SUMS" 2>/dev/null; then
			(cd "$WORK" && grep "ffmpeg-$VERSION.tar.xz" "$HERE/SHA256SUMS" | sha256sum -c -)
		else
			echo "warning: no pinned checksum for ffmpeg-$VERSION.tar.xz; got $(sha256sum "$tarball" | cut -d' ' -f1)" >&2
		fi
		tar -xJf "$tarball" -C "$WORK"
	fi
fi

common=(
	--disable-everything --disable-autodetect --disable-network --disable-doc --disable-debug
	--disable-ffplay --disable-ffprobe --disable-avdevice --disable-swscale --disable-iconv
	--enable-ffmpeg --enable-static --disable-shared --enable-small --enable-pic
	--enable-protocol=file,pipe
	--enable-demuxer=mov,matroska,ogg,aac
	--enable-muxer=mp4,mov,ipod,matroska,webm,ogg,opus,adts
	--enable-parser=h264,hevc,vp9,av1,aac,opus,vorbis
	--enable-bsf=aac_adtstoasc,vp9_superframe,vp9_superframe_split,av1_frame_split,av1_frame_merge,extract_extradata,h264_mp4toannexb,hevc_mp4toannexb,null
	--enable-filter=null,anull,aresample,format,aformat,trim,atrim,setpts,asetpts
)

build=$WORK/build-$TARGET
rm -rf "${build:?}" && mkdir -p "$build" && cd "$build"
strip_bin=strip
case "$TARGET" in
	linux)
		"$SRC/configure" "${common[@]}" ;;
	android-*)
		ndk=${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}
		tc=$ndk/toolchains/llvm/prebuilt/linux-x86_64
		case "$TARGET" in
			android-arm64-v8a) arch=aarch64 cpu=armv8-a triple=aarch64-linux-android ;;
			android-armeabi-v7a) arch=arm cpu=armv7-a triple=armv7a-linux-androideabi ;;
			android-x86_64) arch=x86_64 cpu=x86-64 triple=x86_64-linux-android ;;
			*) echo "unknown target $TARGET" >&2; exit 2 ;;
		esac
		"$SRC/configure" "${common[@]}" \
			--enable-cross-compile --target-os=android --arch="$arch" --cpu="$cpu" \
			--cc="$tc/bin/${triple}${API}-clang" --cxx="$tc/bin/${triple}${API}-clang++" \
			--ar="$tc/bin/llvm-ar" --nm="$tc/bin/llvm-nm" --ranlib="$tc/bin/llvm-ranlib" --strip="$tc/bin/llvm-strip" \
			--sysroot="$tc/sysroot" --extra-ldflags="-pie -Wl,-z,max-page-size=16384"
		strip_bin=$tc/bin/llvm-strip ;;
	*) echo "unknown target $TARGET" >&2; exit 2 ;;
esac
make -j"$JOBS" ffmpeg >/dev/null
"$strip_bin" ffmpeg
case "$TARGET" in
	linux) install -m755 ffmpeg "$OUT/ffmpeg" ;;
	android-*) install -m755 ffmpeg "$OUT/libffmpeg.so" ;;
esac
ls -la "$OUT"
