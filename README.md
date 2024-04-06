**NOTE**
> 因为`rust`的`id3`库没有`python`的`mutegan`那么强大和准确，对于有些`mp3`编码的文件解密会出错，我也不知道如何修复，并且这个程序并不会保留`艺术家`,`曲目`和`专辑`等元信息，但`python`脚本（只是依赖安装稍微有点麻烦，因为好像`python`的`wasmer`在有些（我的）Windows系统下无法运行，但是用`wsl`就可以解决这个问题了。。。）会保留，也不会出错。所以我决定放弃这个程序的修改，转而用`python`脚本来解密。

# xm_decryptor
喜马拉雅下载xm文件解密工具

实现逻辑参考 https://www.aynakeya.com/articles/ctf/xi-ma-la-ya-xm-wen-jian-jie-mi-ni-xiang-fen-xi/

由于xm使用的id3 tag语言位占用2位，不是标准的3位，所以集成了修改的rust-id3代码

由于对python不熟，在windows python 3.11环境下搞了很久没用起来，决定用rust按照原项目逻辑重写一下

编译一个单独的exe文件供朋友们直接使用

# 命令行
xm_decryptor xm文件或目录

# Fork改动
1. 输出文件名的本名与源文件名一样，便于检查是不是全部解密成功。
2. 增加`--dry-run`参数
