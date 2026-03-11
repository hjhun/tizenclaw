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

%description
TizenClaw Native Agent running as a System Service, utilizing LXC for skills execution.

%package unittests
Summary: Unit tests for TizenClaw
Group: System/Service
Requires: %{name} = %{version}-%{release}

%description unittests
Unit tests for TizenClaw

%package rag
Summary: RAG Database for TizenClaw
Group: System/Service
Requires: %{name} = %{version}-%{release}

%description rag
Optional pre-built SQLite Knowledge RAG database for Tizen Docs.

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
mkdir -p %{buildroot}%{_includedir}/tizenclaw
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/img
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/tools/skills
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/config
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/tools/embedded
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/rag
touch %{buildroot}/opt/usr/share/tizenclaw/rag/.keep

ln -sf ../tizenclaw.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw.service
ln -sf ../tizenclaw-skills-secure.service %{buildroot}%{_unitdir}/multi-user.target.wants/tizenclaw-skills-secure.service

%files
%defattr(-,root,root,-)
%manifest %{name}.manifest
%{_bindir}/tizenclaw
%{_bindir}/tizenclaw-cli
%{_bindir}/start_mcp_tunnel.sh
%{_libdir}/libtizenclaw.so.*
%{_libdir}/libtizenclaw-llm-backend.so.*
%{_unitdir}/tizenclaw.service
%{_unitdir}/tizenclaw-skills-secure.service
%{_unitdir}/multi-user.target.wants/tizenclaw.service
%{_unitdir}/multi-user.target.wants/tizenclaw-skills-secure.service
/usr/libexec/tizenclaw/run_standard_container.sh
/usr/libexec/tizenclaw/skills_secure_container.sh
/usr/libexec/tizenclaw/crun
/opt/usr/share/tizenclaw/img/rootfs.tar.gz
/opt/usr/share/tizenclaw/config/*
/opt/usr/share/tizenclaw/tools/skills/
/opt/usr/share/tizenclaw/tools/routing_guide.md
/opt/usr/share/tizenclaw/web/
/opt/usr/share/tizenclaw/tools/embedded/
%dir /opt/usr/share/tizenclaw/tools/
%dir /opt/usr/share/tizenclaw/config/
%dir /opt/usr/share/tizenclaw/
%{_sysconfdir}/package-manager/parserlib/metadata/libtizenclaw-metadata-llm-backend-plugin.so
%{_datarootdir}/parser-plugins/tizenclaw-metadata-llm-backend-plugin.info

%files unittests
%defattr(-,root,root,-)
%manifest %{name}.manifest
%{_bindir}/tizenclaw-unittests

%files rag
%defattr(-,root,root,-)
%manifest %{name}.manifest
/opt/usr/share/tizenclaw/rag/

%files devel
%defattr(-,root,root,-)
%{_includedir}/tizenclaw/
%{_libdir}/libtizenclaw.so
%{_libdir}/libtizenclaw-llm-backend.so
%{_libdir}/pkgconfig/tizenclaw.pc
%{_libdir}/pkgconfig/tizenclaw-llm-backend.pc
