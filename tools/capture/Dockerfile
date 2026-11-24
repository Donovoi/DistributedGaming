# Build and run on a remote Linux PC. This image contains no game or account data.
FROM ubuntu:24.04@sha256:562111b88caa79c71391c0c4d3c2114a7d5c35915c20561fd893cb1e07929e6b

ARG DEBIAN_FRONTEND=noninteractive
RUN dpkg --add-architecture i386 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
       autoconf automake bison build-essential ca-certificates cmake curl \
       dbus-x11 file fontconfig fonts-dejavu-core git \
       libasound2t64 libasound2t64:i386 libatomic1 libatomic1:i386 \
       libc6:i386 libcurl3t64-gnutls libcurl3t64-gnutls:i386 \
       libegl1 libegl1:i386 libgcc-s1:i386 libgl1 libgl1:i386 \
       libnss3 libpulse0 libpulse0:i386 libsdl2-2.0-0 libsdl2-2.0-0:i386 \
       libstdc++6:i386 libtcmalloc-minimal4t64 \
       libvulkan1 libvulkan1:i386 libx11-6:i386 libxss1 libxtst6 \
       libgl1-mesa-dev libpcre3-dev libx11-dev libx11-xcb-dev libxcb-keysyms1-dev \
       mesa-utils pkg-config python3 python3-dev \
       unzip vulkan-tools x11-utils xauth xdotool \
    && rm -rf /var/lib/apt/lists/*

# RenderDoc v1.47. Use the release commit, not a moving branch.
RUN git init /opt/renderdoc-src \
    && git -C /opt/renderdoc-src remote add origin https://github.com/baldurk/renderdoc.git \
    && git -C /opt/renderdoc-src fetch --depth 1 origin b0e1370309f7d7bf986a73830446ca39d9b8c1fd \
    && git -C /opt/renderdoc-src checkout --detach FETCH_HEAD \
    && cmake -S /opt/renderdoc-src -B /opt/renderdoc-build \
       -DCMAKE_BUILD_TYPE=Release -DENABLE_QRENDERDOC=OFF -DENABLE_PYRENDERDOC=ON \
    && cmake --build /opt/renderdoc-build --parallel 8 \
    && cmake --install /opt/renderdoc-build \
    && ldconfig

ENV NVIDIA_DRIVER_CAPABILITIES=compute,utility,graphics,display,compat32
ENV PYTHONPATH=/opt/renderdoc-build/lib
WORKDIR /work
CMD ["bash"]
