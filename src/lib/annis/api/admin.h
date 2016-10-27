#pragma once

#include <string>

namespace annis
{
namespace api
{
  class Admin
  {
  public:
    Admin();
    ~Admin();

    /**
    * Imports data in the relANNIS format to the internal format used by graphANNIS.
    * @param sourceFolder
    * @param targetFolder
    */
   static void importRelANNIS(std::string sourceFolder, std::string targetFolder);
  };
}} // end namespace annis::api
