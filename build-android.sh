#!/bin/bash
# aginxium Android 构建脚本
#
# 前置条件:
#   cargo install cargo-ndk
#   ANDROID_NDK_HOME 环境变量指向 NDK 路径
#
# 用法:
#   ./build-android.sh          # 编译所有架构
#   ./build-android.sh arm64    # 只编译 arm64-v8a
#   ./build-android.sh gen      # 只生成 Kotlin binding（需要先编译）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Android 目标架构
TARGETS=("arm64-v8a" "armeabi-v7a" "x86_64")
NDK_TARGETS=("aarch64-linux-android" "armv7-linux-androideabi" "x86_64-linux-android")

# 检查 cargo-ndk
if ! command -v cargo-ndk &>/dev/null; then
    echo "错误: cargo-ndk 未安装"
    echo "  cargo install cargo-ndk"
    exit 1
fi

# 检查 uniffi-bindgen（通过 cargo run --bin uniffi-bindgen 自带）
# 不需要单独安装

# 检查 NDK
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    # 尝试常见路径
    for p in "$HOME/Android/Sdk/ndk/"* "$HOME/Library/Android/sdk/ndk/"*; do
        if [ -d "$p" ]; then
            export ANDROID_NDK_HOME="$p"
            break
        fi
    done
    if [ -z "${ANDROID_NDK_HOME:-}" ]; then
        echo "错误: ANDROID_NDK_HOME 未设置"
        echo "  export ANDROID_NDK_HOME=/path/to/ndk"
        exit 1
    fi
fi

echo "NDK: $ANDROID_NDK_HOME"

OUTPUT_DIR="$SCRIPT_DIR/target/android"
JNI_LIBS="$OUTPUT_DIR/jniLibs"
BINDINGS_DIR="$OUTPUT_DIR/bindings"

# 添加 Rust target
for t in "${NDK_TARGETS[@]}"; do
    rustup target add "$t" 2>/dev/null || true
done

build() {
    local target="$1"
    echo ""
    echo "── 编译 $target ──"

    cargo ndk -t "$target" \
        -o "$JNI_LIBS" \
        build --release --features ffi
}

generate_bindings() {
    echo ""
    echo "── 生成 Kotlin binding ──"

    # 找到编译产物
    local lib_path=""
    for t in "${TARGETS[@]}"; do
        local candidate="$JNI_LIBS/$t/libaginxium.so"
        if [ -f "$candidate" ]; then
            lib_path="$candidate"
            break
        fi
    done

    if [ -z "$lib_path" ]; then
        # fallback: 用 target/ 下的
        for t in "${NDK_TARGETS[@]}"; do
            local candidate="$SCRIPT_DIR/target/$t/release/libaginxium.so"
            if [ -f "$candidate" ]; then
                lib_path="$candidate"
                break
            fi
        done
    fi

    if [ -z "$lib_path" ]; then
        echo "错误: 找不到 libaginxium.so，请先编译"
        exit 1
    fi

    echo "使用: $lib_path"

    mkdir -p "$BINDINGS_DIR"

    cargo run --features ffi --bin uniffi-bindgen -- \
        generate --library "$lib_path" \
        --language kotlin \
        --out-dir "$BINDINGS_DIR"

    echo "Kotlin binding 已生成到: $BINDINGS_DIR"
}

# 主逻辑
case "${1:-all}" in
    arm64)
        build "arm64-v8a"
        ;;
    arm)
        build "armeabi-v7a"
        ;;
    x86)
        build "x86_64"
        ;;
    gen)
        generate_bindings
        ;;
    all)
        for t in "${TARGETS[@]}"; do
            build "$t"
        done
        generate_bindings
        echo ""
        echo "── 完成 ──"
        echo "jniLibs: $JNI_LIBS"
        echo "bindings: $BINDINGS_DIR"
        ;;
    *)
        echo "用法: $0 [arm64|arm|x86|gen|all]"
        exit 1
        ;;
esac
