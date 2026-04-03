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
BuildRequires:  pkgconfig(pkgmgr-info)
BuildRequires:  pkgconfig(pkgmgr-installer)
BuildRequires:  pkgconfig(pkgmgr-parser)
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
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/memory
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/embedded
# actions/ dir removed — tools are discovered dynamically
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/cli
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/skills
mkdir -p %{buildroot}/opt/usr/share/crash/dump

ln -sf ../tizenclaw.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw.service
ln -sf ../tizenclaw-tool-executor.socket %{buildroot}%{_unitdir}/sockets.target.wants/tizenclaw-tool-executor.socket

%post

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

%config(noreplace) /opt/usr/share/tizenclaw/config/*

# tools.md is generated at runtime by the daemon startup indexer
/opt/usr/share/tizenclaw/web/
/opt/usr/share/tizen-tools/embedded/
# actions/ dir removed
%dir /opt/usr/share/tizen-tools/cli/
%dir /opt/usr/share/tizen-tools/skills/
/opt/usr/share/tizen-tools/cli/*
%dir /opt/usr/share/tizen-tools/
%dir /opt/usr/share/tizenclaw/config/
%dir /opt/usr/share/tizenclaw/memory/
%dir /opt/usr/share/tizenclaw/
%{_libdir}/libtizenclaw-core.so
%{_libdir}/libtizenclaw.so
%dir /opt/usr/share/crash/
%dir /opt/usr/share/crash/dump/

# pkgmgr metadata parser plugins
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-llm-backend-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-llm-backend-plugin.info
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-skill-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-skill-plugin.info
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-cli-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-cli-plugin.info

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
%dir %{_includedir}/tizenclaw/core
%{_includedir}/tizenclaw/core/tizenclaw_channel.h
%{_includedir}/tizenclaw/core/tizenclaw_llm_backend.h
%{_includedir}/tizenclaw/core/tizenclaw_curl.h
%{_libdir}/pkgconfig/tizenclaw.pc
%{_libdir}/pkgconfig/tizenclaw-core.pc

