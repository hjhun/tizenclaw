Name:       tizenclaw
Summary:    TizenClaw Agent System Service App
Version:    1.0.0
Release:    1
Group:      System/Service
License:    Apache-2.0
Source0:    %{name}-%{version}.tar.gz
Source1001: %{name}.manifest
BuildRequires:  cmake
BuildRequires:  pkgconfig(tizen-core)
BuildRequires:  pkgconfig(glib-2.0)
BuildRequires:  pkgconfig(dlog)
BuildRequires:  pkgconfig(libcurl)
BuildRequires:  pkgconfig(gtest)
BuildRequires:  pkgconfig(gmock)
BuildRequires:  pkgconfig(libsoup-2.4)
BuildRequires:  pkgconfig(libwebsockets)
BuildRequires:  pkgconfig(pkgmgr)
BuildRequires:  pkgconfig(pkgmgr-info)
BuildRequires:  pkgconfig(pkgmgr-installer)
BuildRequires:  pkgconfig(pkgmgr-parser)
BuildRequires:  jsoncpp-devel
BuildRequires:  pkgconfig(sqlite3)
BuildRequires:  pkgconfig(capi-appfw-tizen-action)
BuildRequires:  pkgconfig(libaurum)
BuildRequires:  pkgconfig(grpc++)
BuildRequires:  pkgconfig(protobuf)
BuildRequires:  pkgconfig(capi-appfw-event)
BuildRequires:  pkgconfig(capi-system-device)
BuildRequires:  pkgconfig(capi-system-info)
BuildRequires:  pkgconfig(capi-system-runtime-info)
BuildRequires:  pkgconfig(capi-system-system-settings)
BuildRequires:  pkgconfig(capi-system-sensor)
BuildRequires:  pkgconfig(storage)
BuildRequires:  pkgconfig(feedback)
BuildRequires:  pkgconfig(capi-network-wifi-manager)
BuildRequires:  pkgconfig(capi-network-connection)
BuildRequires:  pkgconfig(capi-network-bluetooth)
BuildRequires:  pkgconfig(capi-appfw-app-manager)
BuildRequires:  pkgconfig(capi-appfw-app-control)
BuildRequires:  pkgconfig(capi-appfw-alarm)
BuildRequires:  pkgconfig(capi-appfw-package-manager)
BuildRequires:  pkgconfig(capi-media-sound-manager)
BuildRequires:  pkgconfig(capi-media-tone-player)
BuildRequires:  pkgconfig(notification)
BuildRequires:  pkgconfig(capi-content-media-content)
BuildRequires:  pkgconfig(capi-media-metadata-extractor)
BuildRequires:  pkgconfig(capi-content-mime-type)
BuildRequires:  hal-api-sensor-devel
BuildRequires:  pkgconfig(aul)
BuildRequires:  pkgconfig(rua)
BuildRequires:  pkgconfig(vconf)
BuildRequires:  pkgconfig(vconf-internal-keys)
BuildRequires:  python3-devel
BuildRequires:  python3-base
Requires:       unzip

%description
TizenClaw Native Agent running as a System Service, utilizing LXC for skills execution.

%package unittests
Summary: Unit tests for TizenClaw
Group: System/Service
Requires: %{name} = %{version}-%{release}

%description unittests
Unit tests for TizenClaw


%package devel
Summary: Development files for TizenClaw
Group: Development/Libraries
Requires: %{name} = %{version}-%{release}

%description devel
Development files for TizenClaw (C-API headers and library symlinks).



%prep
%setup -q -n %{name}-%{version}
cp %{SOURCE1001} .

%build
export CFLAGS="$CFLAGS -Wall -Wno-shadow -Wno-unused-function -Os -flto"
export CXXFLAGS="$CXXFLAGS -Wall -Os -flto"
export LDFLAGS="$LDFLAGS -Wl,--as-needed -flto"

%cmake . -DTIZENCLAW_ARCH=%{_arch} -DFULLVER=%{version}
%__make %{?_smp_mflags}


%install
%make_install

# Tizen structure
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_unitdir}
mkdir -p %{buildroot}%{_unitdir}/multi-user.target.wants
mkdir -p %{buildroot}%{_unitdir}/sockets.target.wants
mkdir -p %{buildroot}%{_includedir}/tizenclaw
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/img
# mkdir -p %{buildroot}/opt/usr/share/tizen-tools/skills
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/config
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/embedded
mkdir -p %{buildroot}/opt/usr/share/tizen-tools/cli
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/sandbox/packages/pip
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/sandbox/packages/npm

mkdir -p %{buildroot}/opt/usr/share/tizenclaw/rag
mkdir -p %{buildroot}/opt/usr/share/crash/dump



ln -sf ../tizenclaw.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw.service
ln -sf ../tizenclaw-tool-executor.socket %{buildroot}%{_unitdir}/sockets.target.wants/tizenclaw-tool-executor.socket
ln -sf ../tizenclaw-code-sandbox.socket %{buildroot}%{_unitdir}/sockets.target.wants/tizenclaw-code-sandbox.socket

%post
# Unzip RAG web docs for LLM reference
if [ -f /opt/usr/share/tizenclaw/rag/web.zip ]; then
  mkdir -p /opt/usr/share/tizenclaw/rag/web
  unzip -o -q /opt/usr/share/tizenclaw/rag/web.zip -d /opt/usr/share/tizenclaw/rag/web
fi

%files
%defattr(-,root,root,-)
%manifest %{name}.manifest
%{_bindir}/tizenclaw
%{_bindir}/tizenclaw-cli
%{_bindir}/start_mcp_tunnel.sh
%{_libdir}/libtizenclaw.so.*
%{_libdir}/libtizenclaw-core.so.*
%{_bindir}/tizenclaw-tool-executor
%{_unitdir}/tizenclaw.service
%{_unitdir}/tizenclaw-tool-executor.service
%{_unitdir}/tizenclaw-tool-executor.socket
%{_unitdir}/tizenclaw-code-sandbox.service
%{_unitdir}/tizenclaw-code-sandbox.socket
%{_unitdir}/multi-user.target.wants/tizenclaw.service
%{_unitdir}/sockets.target.wants/tizenclaw-tool-executor.socket
%{_unitdir}/sockets.target.wants/tizenclaw-code-sandbox.socket
/usr/libexec/tizenclaw/run_standard_container.sh
/usr/libexec/tizenclaw/tizenclaw_secure_container.sh
/usr/libexec/tizenclaw/tizenclaw_code_executor.py
/usr/libexec/tizenclaw/crun
/opt/usr/share/tizenclaw/img/rootfs.tar.gz
/opt/usr/share/tizenclaw/config/*
# /opt/usr/share/tizen-tools/skills/
/opt/usr/share/tizen-tools/routing_guide.md
/opt/usr/share/tizen-tools/tools.md
/opt/usr/share/tizenclaw/web/
/opt/usr/share/tizen-tools/embedded/
/opt/usr/share/tizen-tools/cli/
/opt/usr/share/tizen-tools/system_cli/
%dir /opt/usr/share/tizen-tools/
%dir /opt/usr/share/tizenclaw/config/
%dir /opt/usr/share/tizenclaw/sandbox/
%dir /opt/usr/share/tizenclaw/sandbox/packages/
%dir /opt/usr/share/tizenclaw/sandbox/packages/pip/
%dir /opt/usr/share/tizenclaw/sandbox/packages/npm/
%dir /opt/usr/share/tizenclaw/
%dir /opt/usr/share/tizenclaw/rag/
/opt/usr/share/tizenclaw/rag/web.zip

%dir /opt/usr/share/crash/
%dir /opt/usr/share/crash/dump/
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-llm-backend-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-llm-backend-plugin.info
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-skill-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-skill-plugin.info
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-cli-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-cli-plugin.info

%files unittests
%defattr(-,root,root,-)
%manifest %{name}.manifest
%{_bindir}/tizenclaw-unittests

%files devel
%defattr(-,root,root,-)
%{_includedir}/tizenclaw/
%{_libdir}/libtizenclaw.so
%{_libdir}/libtizenclaw-core.so
%{_libdir}/pkgconfig/tizenclaw.pc
%{_libdir}/pkgconfig/tizenclaw-core.pc
