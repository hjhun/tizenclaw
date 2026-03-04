Name:       tizenclaw
Summary:    TizenClaw Agent System Service App
Version:    1.0.0
Release:    1
Group:      System/Service
License:    Apache-2.0
Source0:    %{name}-%{version}.tar.gz
BuildRequires:  cmake
BuildRequires:  pkgconfig(tizen-core)
BuildRequires:  pkgconfig(glib-2.0)
BuildRequires:  pkgconfig(dlog)
BuildRequires:  pkgconfig(libcurl)
BuildRequires:  pkgconfig(gtest)
BuildRequires:  pkgconfig(gmock)
# C++ toolchain is generally available in Tizen

%description
TizenClaw Native Agent running as a System Service Application, utilizing LXC for skills execution.

%prep
%setup -q -n %{name}-%{version}

%build
export CFLAGS="$CFLAGS -Wall"
export CXXFLAGS="$CXXFLAGS -Wall"
export LDFLAGS="$LDFLAGS -Wl,--as-needed"

mkdir -p build
cd build
cmake .. \
    -DCMAKE_INSTALL_PREFIX=/usr
make %{?_smp_mflags}

%check
cd build
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:%{buildroot}/usr/lib
ctest -V

%install
rm -rf %{buildroot}
cd build
%make_install

# Tizen apps structure
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/systemd/system/
mkdir -p %{buildroot}/opt/usr/share/tizenclaw/skills

%files
%defattr(-,root,root,-)
/usr/bin/tizenclaw
/usr/bin/tizenclaw-unittests
/usr/lib/systemd/system/tizenclaw.service
/opt/usr/share/tizenclaw/rootfs.tar.gz
/opt/usr/share/tizenclaw/skills/
%dir /opt/usr/share/tizenclaw/
