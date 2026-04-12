Name:       tizenclaw
Summary:    TizenClaw Agent System Service App
Version:    1.0.0
Release:    3
Group:      System/Service
License:    Apache-2.0
%undefine _debugsource_packages
Source0:    %{name}-%{version}.tar.gz
Source1001: %{name}.manifest

%ifarch armv7hl armv7l armv7el
%global cargo_target_triple arm-unknown-linux-gnueabi
%else
%global cargo_target_triple x86_64-unknown-linux-gnu
%endif

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
%cmake . -DCMAKE_INSTALL_PREFIX=/ -DCARGO_TARGET_TRIPLE=%{cargo_target_triple}
/usr/bin/cmake --build . --verbose

%install
DESTDIR=%{buildroot} /usr/bin/cmake --install . --verbose

%post
systemctl daemon-reload >/dev/null 2>&1 || true
systemctl enable tizenclaw.service >/dev/null 2>&1 || true

%files
%defattr(-,root,root,-)
# package manifest intentionally omitted
%{_bindir}/tizenclaw
%{_bindir}/tizenclaw-cli
%{_bindir}/tizenclaw-tool-executor
%{_bindir}/tizenclaw-web-dashboard
%{_unitdir}/tizenclaw.service
%dir /opt/usr/share/tizenclaw/
/opt/usr/share/tizenclaw/config/
/opt/usr/share/tizenclaw/docs/
/opt/usr/share/tizenclaw/embedded/
/opt/usr/share/tizenclaw/plugins/libtizenclaw_plugin.so
/opt/usr/share/tizenclaw/web/
