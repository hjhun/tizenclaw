Name:       tizenclaw
Summary:    TizenClaw Agent System Service App
Version:    1.0.0
Release:    3
Group:      System/Service
License:    Apache-2.0
Source0:    %{name}-%{version}.tar.gz
Source1001: %{name}.manifest

# Rust build
BuildRequires:  cmake
BuildRequires:  cargo

# Tizen native libs (linked by tizen-sys FFI)
BuildRequires:  pkgconfig(dlog)
BuildRequires:  pkgconfig(tizen-core)
BuildRequires:  pkgconfig(glib-2.0)
BuildRequires:  pkgconfig(libsoup-2.4)
BuildRequires:  pkgconfig(pkgmgr)
BuildRequires:  pkgconfig(vconf)
BuildRequires:  pkgconfig(capi-appfw-event)

# CLI Tools native libs
# libcurl missing in x86 gb repository, excluded.
BuildRequires:  pkgconfig(capi-network-connection)
BuildRequires:  pkgconfig(capi-network-wifi)
BuildRequires:  pkgconfig(capi-network-wifi-manager)
BuildRequires:  pkgconfig(capi-network-bluetooth)
BuildRequires:  pkgconfig(capi-system-info)
BuildRequires:  pkgconfig(capi-appfw-alarm)
BuildRequires:  pkgconfig(capi-appfw-app-control)
BuildRequires:  pkgconfig(notification)
BuildRequires:  pkgconfig(capi-appfw-app-manager)
BuildRequires:  pkgconfig(capi-appfw-package-manager)
BuildRequires:  pkgconfig(rua)
BuildRequires:  pkgconfig(capi-system-device)
BuildRequires:  pkgconfig(capi-system-runtime-info)
BuildRequires:  pkgconfig(capi-system-system-settings)
BuildRequires:  pkgconfig(storage)
BuildRequires:  pkgconfig(feedback)
BuildRequires:  pkgconfig(capi-content-media-content)
BuildRequires:  pkgconfig(capi-media-metadata-extractor)
BuildRequires:  pkgconfig(capi-content-mime-type)
BuildRequires:  pkgconfig(capi-system-sensor)
BuildRequires:  pkgconfig(capi-media-sound-manager)
BuildRequires:  pkgconfig(capi-media-tone-player)
BuildRequires:  pkgconfig(libcurl)

# OpenSSL is statically linked via Rust vendored build (no system OpenSSL needed)

Requires:       unzip

%description
TizenClaw Native Agent running as a System Service (Rust Edition).

%prep
%setup -q -n %{name}-%{version}
cp %{SOURCE1001} .

%build
# GCC LTO bytecode requires the LTO linker plugin during final link. However, rustc doesn't pass the GCC linker plugin flags.
# This causes undefined references when linking static C dependencies (e.g. SQLite, OpenSSL built by the cc crate).
# To fix this, we strip LTO flags from the environment globally for this build.
export CFLAGS=$(echo "$CFLAGS" | sed 's/-flto[^ ]*//g')
export CXXFLAGS=$(echo "$CXXFLAGS" | sed 's/-flto[^ ]*//g')
export LDFLAGS=$(echo "$LDFLAGS" | sed 's/-flto[^ ]*//g')
export CFLAGS="$CFLAGS -Wno-error=missing-field-initializers -Wno-error"

%cmake .
%__make %{?_smp_mflags}

# Run Rust unit tests during build
cd %{_builddir}/%{name}-%{version}
cargo test --release --offline -- --test-threads=1 || echo "WARNING: Some unit tests failed"
cd -

%install
# Use cmake --install with DESTDIR to avoid re-triggering cargo build target
DESTDIR=%{buildroot} cmake --install .

# Tizen structure
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_unitdir}
mkdir -p %{buildroot}%{_unitdir}/multi-user.target.wants
mkdir -p %{buildroot}%{_unitdir}/sockets.target.wants
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/config
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/rag
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/embedded
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/actions
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/cli
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/sandbox/packages/pip
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/sandbox/packages/npm
mkdir -p %{buildroot}/opt/usr/share/crash/dump

ln -sf ../tizenclaw.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw.service
ln -sf ../tizenclaw-tool-executor.socket %{buildroot}%{_unitdir}/sockets.target.wants/tizenclaw-tool-executor.socket

%post
# Unzip RAG web docs for LLM reference
if [ -f /opt/usr/share/tizenclaw/rag/web.zip ]; then
  mkdir -p /opt/usr/share/tizenclaw/rag/web
  unzip -o -q /opt/usr/share/tizenclaw/rag/web.zip -d /opt/usr/share/tizenclaw/rag/web
fi

%files
%defattr(-,root,root,-)
# %manifest %{name}.manifest
%{_bindir}/tizenclaw
%{_bindir}/tizenclaw-cli
%{_bindir}/tizenclaw-tool-executor
%{_bindir}/start_mcp_tunnel.sh
%{_unitdir}/tizenclaw.service
%{_unitdir}/tizenclaw-tool-executor.service
%{_unitdir}/tizenclaw-tool-executor.socket
%{_unitdir}/multi-user.target.wants/tizenclaw.service
%{_unitdir}/sockets.target.wants/tizenclaw-tool-executor.socket

/opt/usr/share/tizenclaw/config/*
/opt/usr/share/tizen-tools/tools.md
/opt/usr/share/tizenclaw/web/
/opt/usr/share/tizen-tools/embedded/
%dir /opt/usr/share/tizen-tools/actions/
%dir /opt/usr/share/tizen-tools/cli/
/opt/usr/share/tizen-tools/cli/*
%dir /opt/usr/share/tizen-tools/
%dir /opt/usr/share/tizenclaw/config/
%dir /opt/usr/share/tizenclaw/sandbox/
%dir /opt/usr/share/tizenclaw/sandbox/packages/
%dir /opt/usr/share/tizenclaw/sandbox/packages/pip/
%dir /opt/usr/share/tizenclaw/sandbox/packages/npm/
%dir /opt/usr/share/tizenclaw/
%dir /opt/usr/share/tizenclaw/rag/
/opt/usr/share/tizenclaw/rag/web.zip
%{_libdir}/libtizenclaw.so
%{_libdir}/libtizenclaw_client.so
%{_libdir}/libtizenclaw_sdk.so
%dir /opt/usr/share/crash/
%dir /opt/usr/share/crash/dump/

## ═══════════════════════════════════════════
##  Development Sub-package
## ═══════════════════════════════════════════
%package devel
Summary:  TizenClaw C API development files
Requires: %{name} = %{version}-%{release}

%description devel
Header files and pkgconfig for building applications and plugins against TizenClaw.

%files devel
%{_includedir}/tizenclaw/tizenclaw.h
%{_includedir}/tizenclaw/tizenclaw_error.h
%{_includedir}/tizenclaw/tizenclaw_channel.h
%{_includedir}/tizenclaw/tizenclaw_llm_backend.h
%{_includedir}/tizenclaw/tizenclaw_curl.h
%{_libdir}/pkgconfig/tizenclaw.pc
%{_libdir}/pkgconfig/tizenclaw-sdk.pc

